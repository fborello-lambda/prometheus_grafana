use axum::{
    extract::Path,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use prometheus::{Encoder, IntCounter, IntCounterVec, Opts, Registry, TextEncoder};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
};
use tokio::time::{self, Duration};

struct AppState {
    transaction_count: Mutex<u64>,
    transactions: Mutex<HashMap<u64, Transaction>>,
}

#[derive(Deserialize, Serialize, Debug)]
struct Transaction {
    operation: String,
    id: u64,
    value: f64,
}

// The metrics are not part of the App state, in this way it's easier to update the metrics,
// and easier to update the app state individually.
pub struct Metrics {
    pub transactions_tracker: Arc<Mutex<IntCounterVec>>,
    pub transactions_total: Arc<Mutex<IntCounter>>,
}

pub enum MetricsTxStatus {
    Failed,
    Succeeded,
}

impl MetricsTxStatus {
    pub fn to_str(&self) -> &str {
        match self {
            MetricsTxStatus::Failed => "failed",
            MetricsTxStatus::Succeeded => "succedded",
        }
    }
}

pub enum MetricsTxType {
    Normal,
    Blobs,
}

impl MetricsTxType {
    pub fn to_str(&self) -> &str {
        match self {
            MetricsTxType::Normal => "normal",
            MetricsTxType::Blobs => "blobs",
        }
    }
}

impl Metrics {
    pub fn new() -> Self {
        Metrics {
            transactions_tracker: Arc::new(Mutex::new(
                IntCounterVec::new(
                    Opts::new(
                        "transactions_tracker",
                        "Keeps track of all transactions depending on status and tx_type",
                    ),
                    &["status", "tx_type"],
                )
                .unwrap(),
            )),
            transactions_total: Arc::new(Mutex::new(
                IntCounter::new("transactions_total", "Keeps track of all transactions").unwrap(),
            )),
        }
    }

    pub fn inc_tx_with_status_and_type(&self, status: MetricsTxStatus, tx_type: MetricsTxType) {
        let txs = self.transactions_tracker.clone();

        let txs_lock = match txs.lock() {
            Ok(lock) => lock,
            Err(e) => {
                tracing::error!("Failed to lock mutex: {e}");
                return;
            }
        };

        let txs_builder =
            match txs_lock.get_metric_with_label_values(&[status.to_str(), tx_type.to_str()]) {
                Ok(builder) => builder,
                Err(e) => {
                    tracing::error!("Failed to build Metric: {e}");
                    return;
                }
            };

        txs_builder.inc();
    }

    pub fn inc_tx(&self) {
        let txs = self.transactions_total.clone();

        let txs_lock = match txs.lock() {
            Ok(lock) => lock,
            Err(e) => {
                tracing::error!("Failed to lock mutex: {e}");
                return;
            }
        };

        txs_lock.inc();
    }

    pub fn gather_metrics(&self) -> String {
        let r = Registry::new();

        let txs_tracker = self.transactions_tracker.clone();
        let txs_tracker_lock = match txs_tracker.lock() {
            Ok(lock) => lock,
            Err(e) => {
                tracing::error!("Failed to lock transactions_tracker mutex: {e}");
                return String::new();
            }
        };

        let txs_lock = self.transactions_total.clone();
        let txs_lock = match txs_lock.lock() {
            Ok(lock) => lock,
            Err(e) => {
                tracing::error!("Failed to lock transactions_total mutex: {e}");
                return String::new();
            }
        };

        if r.register(Box::new(txs_lock.clone())).is_err() {
            tracing::error!("Failed to register metric");
            return String::new();
        }
        if r.register(Box::new(txs_tracker_lock.clone())).is_err() {
            tracing::error!("Failed to register metric");
            return String::new();
        }

        let encoder = TextEncoder::new();
        let metric_families = r.gather();

        let mut buffer = Vec::new();
        if encoder.encode(&metric_families, &mut buffer).is_err() {
            tracing::error!("Failed to encode metrics");
            return String::new();
        }

        String::from_utf8(buffer).unwrap_or_else(|e| {
            tracing::error!("Failed to convert buffer to String: {e}");
            String::new()
        })
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

pub static METRICS: LazyLock<Metrics> = LazyLock::new(Metrics::default);

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let shared_state = Arc::new(AppState {
        transaction_count: Mutex::new(0),
        transactions: Mutex::new(HashMap::new()),
    });

    // Start background task that counts the transactions per second.
    // It gets lost every second.
    let state_clone = Arc::clone(&shared_state);
    tokio::spawn(async move {
        loop {
            time::sleep(Duration::from_secs(1)).await;
            let mut count = state_clone.transaction_count.lock().unwrap();
            *count = 0;
        }
    });

    let app = Router::new()
        .route(
            "/transact",
            post({
                let shared_state = Arc::clone(&shared_state);
                move |body| create_transaction(body, shared_state)
            }),
        )
        .route(
            "/transaction/:id",
            get({
                let shared_state = Arc::clone(&shared_state);
                move |path| get_transaction(path, shared_state)
            }),
        )
        .route(
            "/requests_per_second",
            get({
                let shared_state: Arc<AppState> = Arc::clone(&shared_state);
                move || get_requests_per_second(shared_state)
            }),
        )
        .route("/metrics", get(get_metrics));

    // Start the axum app
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_transaction(Path(id): Path<String>, state: Arc<AppState>) -> String {
    let transactions = state.transactions.lock().unwrap();
    let entry = transactions.get(&id.parse::<u64>().unwrap());
    tracing::info!("get_transaction()");
    match entry {
        Some(transaction) => format!("Transaction: {transaction:#?}"),
        None => "Transaction not found".to_owned(),
    }
}

async fn get_metrics() -> String {
    METRICS.gather_metrics()
}

async fn create_transaction(
    Json(payload): Json<Transaction>,
    state: Arc<AppState>,
) -> impl IntoResponse {
    let status = if rand::thread_rng().gen_bool(0.5) {
        MetricsTxStatus::Succeeded
    } else {
        MetricsTxStatus::Failed
    };

    let tx_type = if rand::thread_rng().gen_bool(0.5) {
        MetricsTxType::Normal
    } else {
        MetricsTxType::Blobs
    };

    METRICS.inc_tx_with_status_and_type(status, tx_type);
    METRICS.inc_tx();

    let mut count = state.transaction_count.lock().unwrap();
    *count += 1;

    let mut transactions = state.transactions.lock().unwrap();
    transactions.entry(payload.id).or_insert(payload);
    tracing::info!("create_transaction()");
    axum::http::StatusCode::CREATED
}

async fn get_requests_per_second(state: Arc<AppState>) -> String {
    let count = state.transaction_count.lock().unwrap();
    format!("Requests per second: {}", *count)
}
