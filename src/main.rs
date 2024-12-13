use axum::{
    extract::Path,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use prometheus::{Encoder, IntCounter, Registry, TextEncoder};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::time::{self, Duration};

struct AppState {
    transaction_count: Mutex<u64>,
    transaction_prom_count: IntCounter,
    transactions: Mutex<HashMap<u64, Transaction>>,
}

#[derive(Deserialize, Serialize, Debug)]
struct Transaction {
    operation: String,
    id: u64,
    value: f64,
}

#[tokio::main]
async fn main() {
    let transaction_prom_count =
        IntCounter::new("transactions_total", "Total number of transactions").unwrap();

    let shared_state = Arc::new(AppState {
        transaction_count: Mutex::new(0),
        transaction_prom_count,
        transactions: Mutex::new(HashMap::new()),
    });

    // Start background task that counts the transactions per second
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
        .route(
            "/metrics",
            get({
                let shared_state: Arc<AppState> = Arc::clone(&shared_state);
                move || get_metrics(shared_state)
            }),
        );

    // Start the axum app
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_transaction(Path(id): Path<String>, state: Arc<AppState>) -> String {
    let transactions = state.transactions.lock().unwrap();
    let entry = transactions.get(&id.parse::<u64>().unwrap());
    match entry {
        Some(transaction) => format!("Transaction: {transaction:#?}"),
        None => "Transaction not found".to_owned(),
    }
}

async fn get_metrics(state: Arc<AppState>) -> String {
    let r = Registry::new();
    r.register(Box::new(state.transaction_prom_count.clone()))
        .unwrap();

    let encoder = TextEncoder::new();
    let metric_families = r.gather();

    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();

    let str = String::from_utf8(buffer).unwrap();
    println!("{str}");

    str
}

async fn create_transaction(
    Json(payload): Json<Transaction>,
    state: Arc<AppState>,
) -> impl IntoResponse {
    state.transaction_prom_count.inc();

    let mut count = state.transaction_count.lock().unwrap();
    *count += 1;

    let mut transactions = state.transactions.lock().unwrap();
    transactions.entry(payload.id).or_insert(payload);
    axum::http::StatusCode::CREATED
}

async fn get_requests_per_second(state: Arc<AppState>) -> String {
    let count = state.transaction_count.lock().unwrap();
    format!("Requests per second: {}", *count)
}
