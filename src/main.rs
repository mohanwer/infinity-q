use std::error::Error;
use crate::server::{TcpServer};

mod queue;
mod resp;
mod server;
mod raw_cmd;
mod constants;
mod utils;

#[tokio::main]
async fn main() {
    let server = TcpServer::new();
    server.start().await.expect("TODO: panic message");
}