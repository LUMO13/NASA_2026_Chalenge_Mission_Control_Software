//! Мікросервіс отримання телеметрії (`telemetry-receiver`).

use std::sync::{Arc, Mutex};
use axum::{extract::State, routing::post, Json, Router};
use tracing::info;

use shared_core::TelemetryEnvelope;
use telemetry_receiver::TelemetryReceiverService;

#[derive(Clone)]
struct ReceiverAppState {
    service: Arc<Mutex<TelemetryReceiverService>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // Standalone network components require an explicitly configured shared secret.
    let hmac_secret = std::env::var("LUNAR_HMAC_SECRET")?.into_bytes();

    let bind_addr = std::env::var("LUNAR_RECEIVER_BIND")
        .unwrap_or_else(|_| "127.0.0.1:7002".to_string());

    let state = ReceiverAppState {
        service: Arc::new(Mutex::new(TelemetryReceiverService::new(hmac_secret, 5000))),
    };

    let app = Router::new()
        .route("/api/v1/receiver/ingest", post(ingest_frame))
        .with_state(state);

    info!("Сервіс отримувача телеметрії запущено на {}", bind_addr);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn ingest_frame(
    State(state): State<ReceiverAppState>,
    Json(envelope): Json<TelemetryEnvelope>,
) -> Json<serde_json::Value> {
    let mut guard = state.service.lock().expect("Receiver mutex poisoned");
    match guard.ingest_and_validate(envelope) {
        Ok(valid_env) => Json(serde_json::json!({
            "status": "accepted",
            "record_id": valid_env.record_id,
            "received_at_utc": valid_env.received_at_utc,
        })),
        Err(err) => Json(serde_json::json!({
            "status": "rejected",
            "error": err.to_string(),
        })),
    }
}
