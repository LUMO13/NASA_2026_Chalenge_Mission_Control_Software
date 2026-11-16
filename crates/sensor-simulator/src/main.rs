//! HTTP/TCP-сервер симулятора апаратних датчиків космонавтів (`sensor-simulator`).

use std::sync::{Arc, Mutex};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use tracing::info;

use sensor_simulator::SensorSimulatorEngine;
use shared_core::{SimulatorControlCommand, TelemetryEnvelope};

#[derive(Clone)]
struct AppState {
    engine: Arc<Mutex<SensorSimulatorEngine>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // Standalone network components require an explicitly configured shared secret.
    let hmac_secret = std::env::var("LUNAR_HMAC_SECRET")?.into_bytes();

    let bind_addr = std::env::var("LUNAR_SIMULATOR_BIND")
        .unwrap_or_else(|_| "127.0.0.1:7001".to_string());

    let state = AppState {
        engine: Arc::new(Mutex::new(SensorSimulatorEngine::new_with_default_crew(
            hmac_secret,
        ))),
    };

    let app = Router::new()
        .route("/api/v1/simulator/poll/:astronaut_id", get(poll_astronaut))
        .route("/api/v1/simulator/poll-all", get(poll_all_astronauts))
        .route("/api/v1/simulator/control", post(apply_control_command))
        .with_state(state);

    info!("Симулятор датчиків ASTRA-MED запущено на {}", bind_addr);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn poll_astronaut(
    State(state): State<AppState>,
    Path(astronaut_id): Path<String>,
) -> Json<Option<TelemetryEnvelope>> {
    let mut guard = state.engine.lock().expect("Simulator mutex poisoned");
    Json(guard.generate_next_envelope(&astronaut_id))
}

async fn poll_all_astronauts(State(state): State<AppState>) -> Json<Vec<TelemetryEnvelope>> {
    let mut guard = state.engine.lock().expect("Simulator mutex poisoned");
    let ids: Vec<String> = guard.astronauts.keys().cloned().collect();
    let mut out = Vec::new();
    for id in ids {
        if let Some(env) = guard.generate_next_envelope(&id) {
            out.push(env);
        }
    }
    Json(out)
}

async fn apply_control_command(
    State(state): State<AppState>,
    Json(cmd): Json<SimulatorControlCommand>,
) -> Json<serde_json::Value> {
    let mut guard = state.engine.lock().expect("Simulator mutex poisoned");
    guard.apply_command(cmd);
    Json(serde_json::json!({ "status": "ok", "running": guard.running }))
}
