//! Мікросервіс розподільника телеметрії та синхронізації із Землею (`telemetry-distributor`).

use std::sync::{Arc, Mutex};
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use tracing::info;

use database::LunarDatabaseRepository;
use telemetry_distributor::{DistributorSettings, TelemetryDistributor};

#[derive(Clone)]
struct DistState {
    repo: Arc<Mutex<LunarDatabaseRepository>>,
    distributor: Arc<Mutex<TelemetryDistributor>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let bind_addr = std::env::var("LUNAR_DISTRIBUTOR_BIND")
        .unwrap_or_else(|_| "127.0.0.1:7003".to_string());

    let state = DistState {
        repo: Arc::new(Mutex::new(LunarDatabaseRepository::new(
            "data/local_offline_buffer.sqlite3",
        ))),
        distributor: Arc::new(Mutex::new(TelemetryDistributor::new(
            DistributorSettings::default(),
        ))),
    };

    let app = Router::new()
        .route("/api/v1/distributor/status", get(get_status))
        .route("/api/v1/distributor/sync-now", post(trigger_manual_sync))
        .with_state(state);

    info!("Розподільник телеметрії ASTRA-MED запущено на {}", bind_addr);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn get_status(State(state): State<DistState>) -> Json<serde_json::Value> {
    let dist = state.distributor.lock().expect("Distributor mutex");
    Json(serde_json::json!({
        "queued_batches": dist.queued_batches_count(),
        "accumulation_window_secs": dist.settings.accumulation_window_secs,
        "earth_link_online": dist.settings.earth_link_online,
    }))
}

async fn trigger_manual_sync(State(state): State<DistState>) -> Json<serde_json::Value> {
    let mut repo = state.repo.lock().expect("Repo mutex");
    let mut dist = state.distributor.lock().expect("Distributor mutex");
    let _ = dist.flush_buffer_to_priority_queue(&mut repo);
    match dist.transmit_next_queued_batch(&mut repo) {
        Ok(Some(ack)) => Json(serde_json::json!({
            "status": "synced",
            "batch_id": ack.batch_id,
            "verified_sha256": ack.verified_sha256,
        })),
        Ok(None) => Json(serde_json::json!({
            "status": "no_batches_or_offline"
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": e.to_string(),
        })),
    }
}
