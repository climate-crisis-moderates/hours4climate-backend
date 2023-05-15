use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, get_service, post},
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;
use std::{collections::HashMap, net::SocketAddr};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

mod config;
use envconfig::Envconfig;
mod countries;
use countries::Country;
mod db;

const RECENT_UPDATES_KEY: &str = "recent_updates";
const MAX_UPDATES: &str = "4";

#[derive(Clone)]
struct AppState {
    redis: redis::Client,
    config: config::Config,
    countries: HashMap<String, Country>,
}

#[tokio::main]
async fn main() {
    let config = config::Config::init_from_env().unwrap();

    // static file mounting
    let static_files_service =
        get_service(ServeDir::new(&config.static_path).append_index_html_on_directories(true))
            .handle_error(|error| async move {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Unhandled internal error: {error}"),
                )
            });

    // set up connection pool
    let redis = db::create_client(&config);

    // initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let (cors, ip) = if config.host_name == "127.0.0.1" {
        (CorsLayer::permissive(), [127, 0, 0, 1])
    } else {
        let origins = [config.host_name.parse().unwrap()];
        (CorsLayer::new().allow_origin(origins), [0, 0, 0, 0])
    };
    let addr = SocketAddr::from((ip, config.http_port));

    let countries = countries::get_countries().await;

    let state = AppState {
        redis,
        config,
        countries,
    };

    // build our application with a route
    let app = Router::new()
        .route("/api/pledge", post(pledge))
        .route("/api/summary", get(summary))
        .route("/api/country", get(country))
        .route("/api/recent", get(recent))
        .with_state(state)
        .fallback(static_files_service)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

#[derive(Deserialize)]
struct SiteKey {
    token: String,
    country: String,
    hours: f32,
}

#[derive(Debug, Deserialize)]
struct SiteKeyResponse {
    success: bool,
}

async fn check_captcha(token: &str, hcaptcha_secret: &str) -> Result<(), (StatusCode, String)> {
    let standard_error = || {
        tracing::error!("Cannot reach hcaptcha");
        (StatusCode::INTERNAL_SERVER_ERROR, "".to_string())
    };

    let client = reqwest::Client::new();
    let response = client
        .post("https://hcaptcha.com/siteverify")
        .form(&[("response", token), ("secret", hcaptcha_secret)])
        .send()
        .await
        .map_err(|_| standard_error())?;
    if response.status() != StatusCode::OK {
        return Err(standard_error());
    }
    let response = response
        .json::<SiteKeyResponse>()
        .await
        .map_err(|_| standard_error())?;
    if !response.success {
        Err((StatusCode::FORBIDDEN, "captcha failed".to_string()))
    } else {
        Ok(())
    }
}

async fn pledge(
    State(state): State<AppState>,
    Json(payload): Json<SiteKey>,
) -> Result<String, (StatusCode, String)> {
    check_captcha(&payload.token, &state.config.hcaptcha_secret).await?;

    if !state.countries.contains_key(&payload.country) {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "country is invalid".to_string(),
        ));
    }

    if payload.hours < 0.0 || payload.hours > 10.0 {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "hours must be >= 0 and <= 10".to_string(),
        ));
    }

    let mut con = state.redis.get_async_connection().await.map_err(|err| {
        tracing::error!("Cannot reach db: {:?}", err);
        (StatusCode::INTERNAL_SERVER_ERROR, "".to_string())
    })?;

    redis::pipe()
        // store the complete dataset
        .cmd("HSET")
        .arg(&[
            &format!("token:{}", payload.token),
            "country",
            &payload.country,
            "hours",
            &payload.hours.to_string(),
            "timestamp",
            &Utc::now().to_rfc3339(),
        ])
        // update the cached sum of country hours
        .cmd("INCRBY")
        .arg(&[
            &format!("country:hours:{}", &payload.country),
            &payload.hours.to_string(),
        ])
        // update the cached count of country persons
        .cmd("INCR")
        .arg(&[&format!("country:count:{}", &payload.country)])
        // push to list of recent updates
        .cmd("LPUSH")
        .arg(&[RECENT_UPDATES_KEY, &payload.token])
        // trim the recent updates
        .cmd("LTRIM")
        .arg(&[RECENT_UPDATES_KEY, "0", MAX_UPDATES])
        .query_async(&mut con)
        .await
        .map_err(|err| {
            tracing::error!("wrong query: {:?}", err);
            (StatusCode::INTERNAL_SERVER_ERROR, "".to_string())
        })?;

    Ok("".to_string())
}

async fn summary(
    State(state): State<AppState>,
) -> Result<Json<Vec<(String, f32, u32)>>, (StatusCode, String)> {
    let mut con = state.redis.get_async_connection().await.map_err(|err| {
        tracing::error!("Cannot reach db: {:?}", err);
        (StatusCode::INTERNAL_SERVER_ERROR, "".to_string())
    })?;

    let countries = state.countries;

    // todo: this can be computed once instead of per request
    let keys = countries
        .keys()
        .map(|c| format!("country:count:{}", c))
        .chain(countries.keys().map(|c| format!("country:hours:{}", c)))
        .collect::<Vec<_>>();

    let count_hours: Vec<Option<String>> = redis::cmd("MGET")
        .arg(&keys)
        .query_async(&mut con)
        .await
        .map_err(|err| {
            tracing::error!("wrong query: {:?}", err);
            (StatusCode::INTERNAL_SERVER_ERROR, "".to_string())
        })?;

    let (count, hours) = count_hours.split_at(countries.len());

    let mut countries = count
        .iter()
        .zip(hours.iter())
        .zip(countries.keys())
        .filter_map(|((count, hours), country)| {
            hours.as_ref().map(|hours| {
                (
                    country.clone(),
                    hours.parse::<f32>().unwrap(),
                    count.as_ref().unwrap().parse::<u32>().unwrap(),
                )
            })
        })
        .collect::<Vec<_>>();

    // sort by country
    countries.sort_unstable_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    Ok(countries.into())
}

async fn recent(
    State(state): State<AppState>,
) -> Result<Json<Vec<(String, f32)>>, (StatusCode, String)> {
    let mut con = state.redis.get_async_connection().await.map_err(|err| {
        tracing::error!("Cannot reach db: {:?}", err);
        (StatusCode::INTERNAL_SERVER_ERROR, "".to_string())
    })?;

    let recent_tokens: Vec<String> = redis::cmd("LRANGE")
        .arg(&[RECENT_UPDATES_KEY, "0", MAX_UPDATES])
        .query_async::<_, Vec<String>>(&mut con)
        .await
        .map_err(|err| {
            tracing::error!("wrong query: {:?}", err);
            (StatusCode::INTERNAL_SERVER_ERROR, "".to_string())
        })?;

    let pipe = recent_tokens
        .into_iter()
        .fold(redis::pipe(), |mut acc, token| {
            acc.cmd("HMGET")
                .arg(&[&format!("token:{token}"), "country", "hours"]);
            acc
        });

    let recent_tokens: Vec<Vec<String>> = pipe.query_async(&mut con).await.map_err(|err| {
        tracing::error!("wrong query: {:?}", err);
        (StatusCode::INTERNAL_SERVER_ERROR, "".to_string())
    })?;
    let recent_tokens = recent_tokens
        .into_iter()
        .map(|mut entry: Vec<String>| {
            let hours = entry
                .pop()
                .unwrap_or_default()
                .parse::<f32>()
                .unwrap_or_default();
            let country = entry.pop().unwrap();
            (country, hours)
        })
        .collect::<Vec<(String, f32)>>();

    Ok(recent_tokens.into())
}

async fn country(State(state): State<AppState>) -> Json<HashMap<String, Country>> {
    state.countries.into()
}
