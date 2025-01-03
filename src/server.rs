use crate::constants::{DEFAULT_CLIENT_SIZE, OKAY_RESPONSE, RESP_BUFFER_SIZE};
use crate::resp_reader::RespReader;
use std::collections::VecDeque;
use std::fmt::Formatter;
use std::string::FromUtf8Error;
use std::{fmt, io};
use tokio::io::{AsyncWriteExt, Error, Interest};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug)]
pub enum SerializeError {
    IncompleteLine,
    MissingContentSize,
    IncompleteCommand,
    UnsupportedTextEncoding,
    UnreadableCommandSize,
}

impl fmt::Display for SerializeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SerializeError::IncompleteLine => {
                write!(f, "End of line not found. Try reading stream again.")
            }
            SerializeError::MissingContentSize => write!(f, "Message does not contain size"),
            SerializeError::IncompleteCommand => write!(f, "Partial read occurred, "),
            SerializeError::UnsupportedTextEncoding => write!(f, "Could not serialize to utf8"),
            SerializeError::UnreadableCommandSize => write!(f, "{}", "Unreadable command size"),
        }
    }
}

impl From<FromUtf8Error> for SerializeError {
    fn from(error: FromUtf8Error) -> Self {
        SerializeError::UnsupportedTextEncoding
    }
}

#[derive(Debug, Clone)]
struct TransmissionMissingArraySize;
impl fmt::Display for TransmissionMissingArraySize {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Message does not contain size")
    }
}

#[derive(Clone, Debug)]
struct TcpClient {
    name: String,
    address: String,
    version: String,
    authenticated: bool,
    msg_from_client: u32,
    msg_cnt_to_client: u32,
    resp_buff_reader: RespReader,
    raw_msg_queue: VecDeque<String>,
}

#[derive(Debug)]
struct BufferReadResult {
    read_to_end_of_message: bool,
    bytes_read: usize,
}

impl TcpClient {
    pub fn new(address: String) -> TcpClient {
        TcpClient {
            name: "unknown".to_string(),
            version: "unknown".to_string(),
            address,
            authenticated: false,
            msg_from_client: 0,
            msg_cnt_to_client: 0,
            resp_buff_reader: RespReader::new(),
            raw_msg_queue: VecDeque::new(),
        }
    }

    pub fn read_buff(
        &mut self,
        buff: [u8; RESP_BUFFER_SIZE],
        buff_bytes: usize,
    ) -> Result<(), SerializeError> {
        let mut bytes_read = 0;
        while bytes_read < buff_bytes {
            bytes_read += self.resp_buff_reader.read(bytes_read, buff_bytes, buff)?;
            if self.resp_buff_reader.reached_end_of_msg {
                let msg_utf8: String = self.resp_buff_reader.write_to_utf8()?;
                self.msg_from_client += 1;
                self.raw_msg_queue.push_back(msg_utf8);
                self.resp_buff_reader.reset();
            }
        }
        Ok(())
    }
}

pub struct TcpServer {
    redis_clients: Vec<TcpClient>,
}

impl TcpServer {
    pub fn new() -> TcpServer {
        TcpServer {
            redis_clients: Vec::with_capacity(DEFAULT_CLIENT_SIZE),
        }
    }

    pub async fn start(&self) -> Result<(), Error> {
        let listener = TcpListener::bind("127.0.0.1:6379").await?;

        match listener.accept().await {
            Ok((stream, _)) => {
                self.handle_stream(stream).await?;
            }
            Err(e) => println!("couldn't get client {:?}", e),
        }

        Ok(())
    }

    async fn handle_stream(&self, mut stream: TcpStream) -> Result<(), Error> {
        let mut okay_sent = false;
        let mut commands_to_process: VecDeque<Vec<Vec<u8>>> = VecDeque::new();
        let mut prev_eol_found = false;
        loop {
            let ready = stream.ready(Interest::READABLE).await?;
            stream.writable().await?;

            if ready.is_readable() {
                let mut data = [0; 4000];
                match stream.try_read(&mut data) {
                    Ok(0) => break,
                    Ok(_) => {
                        if !okay_sent {
                            stream.write_all(OKAY_RESPONSE.as_bytes()).await?;
                            okay_sent = true
                        } else {
                            stream.write_all("+OK\r\n".as_bytes()).await?;
                        }
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
        println!("stream ended");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::create_buffer;
    use crate::utils::get_eol_index;

    #[test]
    fn test_find_next_cr() {
        let buff = &create_buffer();
        let result = get_eol_index(1, &buff).unwrap();
        let expected = 3;
        assert_eq!(result, expected);
    }

    // #[test]
    // fn test_client_buffer_process() {
    //     let mut client = TcpClient::new("0.0.0.0".to_string());
    //     let chunked_buffers = create_chunked_transmission();
    //     for chunk in chunked_buffers.into_iter() {
    //         client.read_buff(&chunk).unwrap();
    //     }
    //     let expected: u32 = 3;
    //     assert_eq!(client.msg_from_client, expected);
    // }
}
