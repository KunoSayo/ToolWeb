use std::collections::HashSet;
use std::io::{stdin};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use axum::extract::DefaultBodyLimit;
use axum::Router;
use axum::routing::{get, post};
use tokio::sync::broadcast;
use tower_http::limit::RequestBodyLimitLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::chat::{chat, websocket_handler};
use crate::upload::{accept_form, save_request_body, show_form};

mod chat;
mod upload;

const UPLOADS_DIRECTORY: &str = "uploads";


// Our shared state
pub struct AppState {
    // We require unique usernames. This tracks which usernames have been taken.
    pub user_set: Mutex<HashSet<String>>,
    // Channel used to send messages to all connected clients.
    pub tx: broadcast::Sender<String>,
}

async fn index() -> &'static str {
    tracing::trace!("get /");
    "Hello World"
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tool_web=trace".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tokio::fs::create_dir_all(UPLOADS_DIRECTORY)
        .await
        .expect("failed to create `uploads` directory");

    // Set up application state for use with with_state().
    let user_set = Mutex::new(HashSet::new());
    let (tx, _rx) = broadcast::channel(100);

    let app_state = Arc::new(AppState { user_set, tx });

    let app = Router::new()
        .route("/", get(index))
        .route("/chat", get(chat))
        .route("/websocket", get(websocket_handler))
        .route("/upload", get(show_form).post(accept_form))
        .route("/file/:file_name", post(save_request_body))
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(2 * 1024 * 1024 * 1024))
        .with_state(app_state);

    let port = loop {
        println!("Please input the port:");
        let mut buf = String::new();
        stdin().read_line(&mut buf).expect("Read port failed");
        match u16::from_str(buf.trim()) {
            Ok(x) => break x,
            Err(e) => {
                eprintln!("Parse port failed for {:?}", e);
            }
        }
    };
    let v6 = {
        let app = app.clone();
        tokio::spawn(async move {
            let addr = SocketAddr::from_str(&format!("[::]:{port}")).unwrap();
            tracing::debug!("listening on {}", addr);
            axum::Server::bind(&addr)
                .serve(app.into_make_service())
                .await
                .unwrap();
        })
    };
    let v4 = tokio::spawn(async move {
        let addr = SocketAddr::from_str(&format!("0.0.0.0:{port}")).unwrap();
        tracing::debug!("listening on {}", addr);
        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
            .unwrap();
    });
    futures::join!(v4, v6);


    Ok(())
}
