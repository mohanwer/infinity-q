use crate::constants::{DEFAULT_CLIENT_SIZE, OKAY_RESPONSE};
use crate::resp_buffered_reader::{RespBuffReadResult, RespBufferedReader};
use crate::utils::get_zero_byte_index;
use std::collections::VecDeque;
use std::fmt::Formatter;
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
    cmds_from_client: u32,
    msg_cnt_to_client: u32,
    raw_msg_queue: VecDeque<Vec<String>>,
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
            cmds_from_client: 0,
            msg_cnt_to_client: 0,
            raw_msg_queue: VecDeque::new(),
        }
    }

    pub fn read_buff(
        &self,
        buff_reader: &mut RespBufferedReader,
        buff: &[u8],
    ) -> Result<RespBuffReadResult, SerializeError> {
        let mut read_result = RespBuffReadResult::new();
        while read_result.end_of_message_reached {
            read_result = buff_reader.read_line(&buff)?;
        }
        Ok(read_result)
    }

    pub fn read_msgs(&self, buff: &[u8]) -> Result<Vec<RespBufferedReader>, SerializeError> {
        let mut readers = Vec::new();
        let mut bytes_read: usize = 0;
        let mut reader = RespBufferedReader::new();
        let end_of_buffer = get_zero_byte_index(0, buff);
        while bytes_read < end_of_buffer {
            let read_result = self.read_buff(&mut reader, buff)?;
            bytes_read += read_result.bytes_read;
        }
        Ok(readers)
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
    use crate::resp_buffered_reader::RespBufferedReader;
    use crate::server::TcpClient;
    use crate::utils::get_eol_index;

    const FIRST_LINE_IDX: usize = 1;
    const SECOND_LINE_IDX: usize = 5;

    fn create_buffer() -> Vec<u8> {
        //    Corresponds to ASCII code
        //   *   5   \r  \n  $   5   \r  \n   h    e    l    l    o  \r  \n
        vec![
            42, 53, 13, 10, 36, 53, 13, 10, 104, 101, 108, 108, 11, 13, 10,
        ]
    }

    fn create_hello() -> Vec<u8> {
        vec![
            42, 53, 13, 10, // *5
            36, 53, 13, 10, // $5
            104, 101, 108, 108, 111, 13, 10, // hello
            36, 49, 13, 10, // $1
            51, 13, 10, // 3
            36, 52, 13, 10, // $4
            97, 117, 116, 104, 13, 10, // auth
            36, 52, 13, 10, // $4
            114, 111, 111, 116, 13, 10, // root
            36, 51, 13, 10, // $3
            97, 98, 99, 13, 10, // abc
        ]
    }

    fn create_set_info() -> Vec<u8> {
        vec![
            42, 52, 13, 10, // *4
            36, 54, 13, 10, // $6
            99, 108, 105, 101, 110, 116, 13, 10, // client
            36, 55, 13, 10, // $7
            115, 101, 116, 105, 110, 102, 111, 13, 10, // setinfo
            36, 56, 13, 10, // $8
            76, 73, 66, 45, 78, 65, 77, 69, 13, 10, // LIB-NAME
            36, 49, 57, 13, 10, // $19
            103, 111, 45, 114, 101, 100, 105, 115, 40, 44, 103, 111, 49, 46, 50, 50, 46, 55, 41,
            13, 10, // go-redis(,go1.22.7)
            42, 52, 13, 10, // *4
            36, 54, 13, 10, // $6
            99, 108, 105, 101, 110, 116, 13, 10, // client
            36, 55, 13, 10, // $7
            115, 101, 116, 105, 110, 102, 111, 13, 10, // setinfo
            36, 55, 13, 10, // $7
            76, 73, 66, 45, 86, 69, 82, 13, 10, // LIB-VER
            36, 53, 13, 10, // $5
            57, 46, 54, 46, 49, 13, 10, // 9.6.1
        ]
    }

    fn create_ping() -> Vec<u8> {
        vec![
            42, 49, 13, 10, // *1
            36, 52, 13, 10, // $4
            112, 105, 110, 103, 13, 10, // ping
        ]
    }

    fn create_lpush_and_sadd_cmds() -> Vec<u8> {
        vec![
            42, 53, 13, 10, // *5
            36, 53, 13, 10, // $5
            108, 112, 117, 115, 104, 13, 10, // lpush
            36, 52, 13, 10, // $4
            107, 101, 121, 49, 13, 10, // key1
            36, 53, 13, 10, // $5
            118, 97, 108, 117, 101, 13, 10, // value
            36, 49, 13, 10, // $1
            55, 13, 10, // 7
            36, 49, 13, 10, // $1
            56, 13, 10, // 8
            42, 51, 13, 10, // *3
            36, 52, 13, 10, // $4
            115, 97, 100, 100, 13, 10, // sadd
            36, 52, 13, 10, // $4
            107, 101, 121, 50, 13, 10, // key 2
            36, 54, 13, 10, // $6
            118, 97, 108, 117, 101, 51, 13, 10, // value3
            42, 53, 13, 10, // *5
            36, 53, 13, 10, // $5
            108, 112, 117, 115, 104, 13, 10, // lpush
            36, 52, 13, 10, // $4
            107, 101, 121, 51, 13, 10, // key 3
            36, 53, 13, 10, // $5
            118, 97, 108, 117, 101, 13, 10, // value
            36, 49, 13, 10, // $1
            55, 13, 10, // 7
            36, 49, 13, 10, // $1
            56, 13, 10, // 8
        ]
    }

    fn create_chunked_transmission() -> Vec<Vec<u8>> {
        vec![
            vec![
                42, 53, 13, 10, // *5
                36, 53, 13, 10, // $5
                108, 112, 117, // lpush
                0, 0, 0, 0, 0, 0,
            ],
            vec![
                115, 104, 13, 10, // lpush
                36, 52, 13, 10, // $4
                107, 101, 121, 49, 13, 10, // key1
                36, 53, 13, 10, // $5
                118, 97, 108, 0, 0, 0, 0, 0, 0,
            ],
            vec![
                117, 101, 13, 10, // value
                36, 49, 13, 10, // $1
                55, 13, 10, // 7
                36, 49, 13, 10, // $1
                56, 13, 10, // 8
                0, 0, 0, 0, 0, 0,
            ],
            vec![
                42, 51, 13, 10, // *3
                36, 52, 13, 10, // $4
                115, 97, 100, 100, 13, 10, // sadd
                36, 52, 13, 10, // $4
                107, 101, 121, 50, 13, 10, // key 2
                36, 54, 13, 10, // $6
                118, 97, 108, 117, 101, 51, 13, 10, // value3
                42, 53, 13, 10, // *5
                36, 53, 13, 10, // $5
                108, 112, 117, 115, 104, 13, 10, // lpush
                0, 0, 0, 0, 0, 0,
            ],
            vec![
                36, 52, 13, 10, // $4
                107, 101, 121, 51, 13, 10, // key 3
                36, 53, 13, 10, // $5
                118, 97, 108, 117, 101, 13, 10, // value
                36, 49, 13, 10, // $1
                55, 13, 10, // 7
                36, 49, 13, 10, // $1
                56, 13, 10, 0, // 8
            ],
        ]
    }

    #[test]
    fn test_find_next_cr() {
        let buff = &create_buffer();
        let result = get_eol_index(1, &buff).unwrap();
        let expected = 3;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_client_buffer_process() {
        let mut client = TcpClient::new("0.0.0.0".to_string());
        let chunked_buffers = create_chunked_transmission();
        let mut raw_cmd = RespBufferedReader::new();
        for chunk in chunked_buffers.into_iter() {
            &client.read_buff(&mut raw_cmd, &chunk).unwrap();
        }
        let expected: u32 = 3;
        assert_eq!(client.cmds_from_client, expected);
    }
}
