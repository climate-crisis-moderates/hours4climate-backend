use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, get_service, post},
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
use envconfig::Envconfig;
mod countries;
mod db;

#[derive(Clone)]
struct AppState {
    redis: redis::Client,
    config: config::Config,
    countries: (HashSet<String>, Vec<String>),
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
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let origins = [config.host_name.parse().unwrap()];
    let addr = SocketAddr::from(([0, 0, 0, 0], config.http_port));

    let countries = countries::get_countries().await;

    let state = AppState {
        redis,
        config,
        countries,
    };

    let cors = CorsLayer::new().allow_origin(origins);

    // build our application with a route
    let app = Router::new()
        .route("/api/pledge", post(pledge))
        .route("/api/summary", get(summary))
        .route("/api/country", get(country))
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
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Cannot reach hcaptcha".to_string(),
        )
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

    if !state.countries.0.contains(&payload.country) {
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

    let mut con = state.redis.get_async_connection().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Cannot reach db".to_string(),
        )
    })?;

    redis::pipe()
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
        .cmd("INCRBY")
        .arg(&[
            &format!("country:hours:{}", &payload.country),
            &payload.hours.to_string(),
        ])
        .cmd("INCR")
        .arg(&[&format!("country:count:{}", &payload.country)])
        .query_async(&mut con)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Cannot reach db".to_string(),
            )
        })?;

    Ok("".to_string())
}

async fn summary(
    State(state): State<AppState>,
) -> Result<axum::extract::Json<Vec<(String, f32, u32)>>, (StatusCode, String)> {
    let mut con = state.redis.get_async_connection().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Cannot reach db".to_string(),
        )
    })?;

    let countries = state.countries.1;

    // todo: this can be computed once instead of per request
    let keys = countries
        .iter()
        .map(|c| format!("country:count:{c}"))
        .chain(countries.iter().map(|c| format!("country:hours:{c}")))
        .collect::<Vec<_>>();

    let count_hours: Vec<Option<String>> = redis::cmd("MGET")
        .arg(&keys)
        .query_async(&mut con)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Incorrect query".to_string(),
            )
        })?;

    let (count, hours) = count_hours.split_at(countries.len());

    let mut countries = count
        .iter()
        .zip(hours.iter())
        .zip(countries.into_iter())
        .filter_map(|((count, hours), country)| {
            hours.as_ref().map(|hours| {
                (
                    country,
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

async fn country(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> axum::extract::Json<Vec<String>> {
    let country = params.get("name").map(|c| c.to_lowercase());

    if let Some(country) = country {
        // simple search (does not take non-ascii characters into account and stuff, but well)
        state
            .countries
            .1
            .iter()
            .filter(|c| c.to_lowercase().contains(&country))
            .cloned()
            .collect::<Vec<String>>()
            .into()
    } else {
        state.countries.1.into()
    }
}
