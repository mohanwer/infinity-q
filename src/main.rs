use std::error::Error;
use crate::server::start_ws_server;

mod queue;
mod resp;
mod server;


#[tokio::main]
async fn main() {
    start_ws_server().await.expect("TODO: panic message");
}