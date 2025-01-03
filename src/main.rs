use crate::server::TcpServer;

mod constants;
mod queue;
mod resp;
mod resp_buffered_reader;
mod resp_reader;
mod server;
mod test_utils;
mod utils;

#[tokio::main]
async fn main() {
    let server = TcpServer::new();
    server.start().await.expect("TODO: panic message");
}
