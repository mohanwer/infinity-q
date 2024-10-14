use std::io;
use std::io::Error;
use tokio::io::{AsyncWriteExt, Interest};
use tokio::net::{TcpListener, TcpStream};


const OKAY_RESPONSE :&str = "%7\r\n\
+server\r\n\
+infinity_q\r\n\
+version\r\n\
:1\r\n\
+proto\r\n\
:3\r\n\
+id\r\n\
$1\r\n\
a\r\n\
+mode\r\n\
$10\r\n\
standalone\r\n\
+role\r\n\
$6\r\n\
master\r\n\
+modules\r\n\
*-1\r\n";

const DEFAULT_CLIENT_SIZE = 1000;

struct RedisClient {
    name: str,
    address: str,
    version: str,
    authenticated: bool,
    msg_cnt_from_client: u32,
    msg_cnt_to_client: u32
}


struct TcpServer {
    redis_clients: Vec<RedisClient>
}

impl TcpServer {
    fn new() -> TcpServer {
        TcpServer {
            redis_clients: Vec::with_capacity(DEFAULT_CLIENT_SIZE)
        }
    }
}

pub async fn start_ws_server() -> Result<(), Error> {
    let listener = TcpListener::bind("127.0.0.1:6379").await?;

    match listener.accept().await {
        Ok((stream, addr)) => {
            println!("new client {:?}", addr);
            handle_stream(stream).await?;
        },
        Err(e) => println!("couldn't get client {:?}", e)
    }

    Ok(())
}

pub async fn handle_stream(mut stream: TcpStream) -> Result<(), Error> {
    loop {
        let ready = stream.ready(Interest::READABLE).await?;
        stream.writable().await?;

        if ready.is_readable() {
            let mut data = vec![0; 1024];
            match stream.try_read(&mut data) {
                Ok(0) => break,
                Ok(n) => {
                    println!("read {} bytes", n);
                    let input = String::from_utf8(data);
                    if input.is_ok() {
                        let parsed = input.unwrap();
                        println!("received: {}", parsed);
                    }

                    stream.write_all(OKAY_RESPONSE.as_bytes()).await?;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => {
                    return Err(e.into());
                }
            }
        }
    }

    Ok(())
}

