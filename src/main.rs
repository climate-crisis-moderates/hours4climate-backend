use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, get_service, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, net::SocketAddr};
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
    let redis = db::connect(&config).unwrap();

    // initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let addr = SocketAddr::from(([127, 0, 0, 1], config.http_port));

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
        .with_state(state)
        .fallback(static_files_service)
        .layer(CorsLayer::permissive())
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
            &format!("country_counter:{}", &payload.country),
            &payload.hours.to_string(),
        ])
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

#[derive(Serialize)]
struct SummaryResponse {
    countries: Vec<String>,
    counts: Vec<Option<String>>,
}

async fn summary(
    State(state): State<AppState>,
) -> Result<axum::extract::Json<SummaryResponse>, (StatusCode, String)> {
    let mut con = state.redis.get_async_connection().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Cannot reach db".to_string(),
        )
    })?;

    // todo: this can be computed once instead of per request
    let keys = state
        .countries
        .1
        .iter()
        .map(|c| format!("country_counter:{c}"))
        .collect::<Vec<_>>();

    let counts: Vec<Option<String>> = redis::cmd("MGET")
        .arg(&keys)
        .query_async(&mut con)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Incorrect query".to_string(),
            )
        })?;

    Ok(SummaryResponse {
        // todo: this clone could be avoided
        countries: state.countries.1.clone(),
        counts,
    }
    .into())
}
