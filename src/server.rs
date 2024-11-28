use std::collections::VecDeque;
use std::{fmt, io, str};
use std::fmt::Formatter;
use std::str::Utf8Error;
use tokio::io::{AsyncWriteExt, Interest, Error};
use tokio::net::{TcpListener, TcpStream};
use crate::constants::{ASCII_ASTERISK, ASCII_CARRIAGE_RETURN, ASCII_LINE_FEED, DEFAULT_CLIENT_SIZE};
use crate::raw_cmd::RawCmd;

#[derive(Debug)]
pub enum SerializeError {
    IncompleteLine,
    MissingContentSize,
    IncompleteCommand,
    UnsupportedTextEncoding(Utf8Error),
    UnreadableCommandSize
}

impl fmt::Display for SerializeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SerializeError::IncompleteLine => write!(f, "End of line not found. Try reading stream again."),
            SerializeError::MissingContentSize => write!(f, "Message does not contain size"),
            SerializeError::IncompleteCommand => write!(f, "Partial read occurred, "),
            SerializeError::UnsupportedTextEncoding(e) => write!(f, "{}", e),
            SerializeError::UnreadableCommandSize => write!(f, "{}", "Unreadable command size")
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


pub enum Cmd {
    LPUSH { key: String, elements: Vec<String> },
    LPOP { key: String, count: u32 },
    HELLO { auth: String, password: String },
    SADD { key: String, member: Vec<String> },
    Unknown
}

struct TcpClient {
    name: String,
    address: String,
    version: String,
    authenticated: bool,
    msg_cnt_from_client: u32,
    msg_cnt_to_client: u32,
    raw_msg_queue: VecDeque<RawCmd>,
}

impl TcpClient {
    pub fn new(address: String) -> TcpClient {
        TcpClient {
            name: "unknown".to_string(),
            version: "unknown".to_string(),
            address,
            authenticated: false,
            msg_cnt_from_client: 0,
            msg_cnt_to_client: 0,
            raw_msg_queue: VecDeque::new(),
        }
    }

    pub fn process_buffer(&mut self, buffer: &Vec<u8>, buffer_end: usize) {
        let mut read_idx_start: usize = 0;
        if self.raw_msg_queue.len() == 0 {
            self.raw_msg_queue.push_front(RawCmd::new());
        }

        while read_idx_start < buffer_end {
            let eol_idx_result = get_eol_index(read_idx_start, &buffer, buffer_end);
            let eol_exists = eol_idx_result.is_ok();
            let mut read_idx_end: usize = eol_idx_result.unwrap_or_else(|_| buffer_end);
            let line = buffer[read_idx_start..=read_idx_end].to_vec();
            let last_raw_msg = self.raw_msg_queue.back_mut().unwrap();
            if !last_raw_msg.eol_exists {
                last_raw_msg.data.extend(&line);
                last_raw_msg.eol_exists = eol_exists;
            } else {
                self.raw_msg_queue.push_back(RawCmd {
                    data: line,
                    eol_exists
                });
            }
            read_idx_start = get_next_line_idx(read_idx_end, &buffer);
        }
    }

    pub fn try_serialize_raw_msg(&mut self) -> Result<u16, SerializeError> {
        if self.raw_msg_queue.len() == 0 { return Ok(0); }
        let raw_msg_first_line = self.raw_msg_queue.front().unwrap();
        if raw_msg_first_line.data.len() == 0 {
            return Err(SerializeError::IncompleteCommand);
        }
        let msg_data_type = raw_msg_first_line.data[0];
        if msg_data_type != ASCII_ASTERISK {
            return Err(SerializeError::IncompleteLine);
        }
        let msg = str::from_utf8(&raw_msg_first_line.data[1..]).unwrap();
        let expect_msg_size_result = msg.parse::<u16>();
        if expect_msg_size_result.is_err() {
            return Err(SerializeError::IncompleteLine);
        }
        let expected_msg_size = expect_msg_size_result.unwrap() * 2;
        let mut actual_msg_size = 0;
        for msg in &self.raw_msg_queue {
            if msg.data[0] == ASCII_CARRIAGE_RETURN {
                actual_msg_size += 1;
            }
            if actual_msg_size == expected_msg_size { break; }
        }
        if actual_msg_size < expected_msg_size { return Err(SerializeError::IncompleteCommand) }
        Ok(expected_msg_size)
    }
}

pub struct TcpServer {
    redis_clients: Vec<TcpClient>
}

impl TcpServer {
    pub fn new() -> TcpServer {
        TcpServer {
            redis_clients: Vec::with_capacity(DEFAULT_CLIENT_SIZE)
        }
    }

    pub async fn start(&self) -> Result<(), Error> {
        let listener = TcpListener::bind("127.0.0.1:6379").await?;

        match listener.accept().await {
            Ok((stream, addr)) => {
                self.handle_stream(stream).await?;
            },
            Err(e) => println!("couldn't get client {:?}", e)
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
                let mut data = vec![0; 4000];

                match stream.try_read(&mut data) {
                    Ok(0) => break,
                    Ok(n) => {
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

fn is_end_of_line_index(index: usize, buff: &Vec<u8>) -> bool {
    index < buff.len() &&
        2 <= index &&
        buff[index] == ASCII_LINE_FEED &&
        buff[index-1] == ASCII_CARRIAGE_RETURN
}

fn get_next_line_idx(eol_index: usize, buff: &Vec<u8>) -> usize {
    let mut end = eol_index;
    while end < buff.len() && (buff[end] == ASCII_LINE_FEED || buff[end] == ASCII_CARRIAGE_RETURN) {
        end += 1;
    }
    end
}

fn get_eol_index(start: usize, buff: &Vec<u8>, buffer_end: usize) -> Result<usize, SerializeError> {
    let mut end = start;

    while end <= buffer_end && !is_end_of_line_index(end, buff)  {
        end += 1;
    }

    if !is_end_of_line_index(end, buff) {
        return Err(SerializeError::IncompleteLine)
    }

    Ok(end)
}

#[cfg(test)]
mod tests {
    use crate::server::{get_eol_index, TcpClient};

    const FIRST_LINE_IDX: usize = 1;
    const SECOND_LINE_IDX: usize = 5;

    fn create_buffer() -> Vec<u8> {
        //    Corresponds to ASCII code
        //   *   5   \r  \n  $   5   \r  \n   h    e    l    l    o  \r  \n
        vec![42, 53, 13, 10, 36, 53, 13, 10, 104, 101, 108, 108, 11, 13, 10]
    }
    
    fn create_hello() -> Vec<u8> {
        vec![
            42,53,13,10,            // *5
            36,53,13,10,            // $5
            104,101,108,108,111,13,10, // hello
            36,49,13,10,            // $1
            51,13,10,               // 3
            36,52,13,10,            // $4
            97,117,116,104,13,10,   // auth
            36,52,13,10,            // $4
            114,111,111,116,13,10,  // root
            36,51,13,10,            // $3
            97,98,99,13,10          // abc
        ]
    }

    fn create_set_info() -> Vec<u8> {
        vec![
            42,52,13,10,    // *4
            36,54,13,10,    // $6
            99,108,105,101,110,116,13,10, // client
            36,55,13,10,    // $7
            115,101,116,105,110,102,111,13,10, // setinfo
            36,56,13,10,    // $8
            76,73,66,45,78,65,77,69,13,10, // LIB-NAME
            36,49,57,13,10, // $19
            103,111,45,114,101,100,105,115,40,44,103,111,49,46,50,50,46,55,41,13,10, // go-redis(,go1.22.7)
            42,52,13,10,    // *4
            36,54,13,10,    // $6
            99,108,105,101,110,116,13,10, // client
            36,55,13,10,    // $7
            115,101,116,105,110,102,111,13,10, // setinfo
            36,55,13,10,    // $7
            76,73,66,45,86,69,82,13,10, // LIB-VER
            36,53,13,10,    // $5
            57,46,54,46,49,13,10 // 9.6.1
        ]
    }

    fn create_ping() -> Vec<u8> {
        vec![
            42,49,13,10,            // *1
            36,52,13,10,            // $4
            112,105,110,103,13,10   // ping
        ]
    }

    fn create_lpush_and_sadd_cmds() -> Vec<u8> {
        vec![
            42,53,13,10, // *5
            36,53,13,10, // $5
            108,112,117,115,104,13,10, // lpush
            36,52,13,10, // $4
            107,101,121,49,13,10, // key1
            36,53,13,10, // $5
            118,97,108,117,101,13,10, // value
            36,49,13,10, // $1
            55,13,10, // 7
            36,49,13,10, // $1
            56,13,10, // 8
            42,51,13,10, // *3
            36,52,13,10, // $4
            115,97,100,100,13,10, // sadd
            36,52,13,10, // $4
            107,101,121,50,13,10, // key 2
            36,54,13,10, // $6
            118,97,108,117,101,51,13,10, // value3
            42,53,13,10, // *5
            36,53,13,10, // $5
            108,112,117,115,104,13,10, // lpush
            36,52,13,10, // $4
            107,101,121,51,13,10, // key 3
            36,53,13,10, // $5
            118,97,108,117,101,13,10, // value
            36,49,13,10, // $1
            55,13,10, // 7
            36,49,13,10, // $1
            56,13,10 // 8
        ]
    }

    fn create_chunked_transmission() -> Vec<Vec<u8>> {
        vec![
            vec![
                42,53,13,10, // *5
                36,53,13,10, // $5
                108,112,117, // lpush
                0,0,0,0,0,0
            ],
            vec![
                115,104,13,10, // lpush
                36,52,13,10, // $4
                107,101,121,49,13,10, // key1
                36,53,13,10, // $5
                118,97,108,
                0,0,0,0,0,0,
            ],
            vec![
                117,101,13,10, // value
                36,49,13,10, // $1
                55,13,10, // 7
                36,49,13,10, // $1
                56,13,10, // 8
                0,0,0,0,0,0,
            ],
            vec![
                42,51,13,10, // *3
                36,52,13,10, // $4
                115,97,100,100,13,10, // sadd
                36,52,13,10, // $4
                107,101,121,50,13,10, // key 2
                36,54,13,10, // $6
                118,97,108,117,101,51,13,10, // value3
                42,53,13,10, // *5
                36,53,13,10, // $5
                108,112,117,115,104,13,10, // lpush
                0,0,0,0,0,0,
            ],
            vec![
                36,52,13,10, // $4
                107,101,121,51,13,10, // key 3
                36,53,13,10, // $5
                118,97,108,117,101,13,10, // value
                36,49,13,10, // $1
                55,13,10, // 7
                36,49,13,10, // $1
                56,13,10,0 // 8
            ]
        ]
    }

    #[test]
    fn test_find_next_cr() {
        let buff = &create_buffer();
        let buff_end = buff.len();
        let result = get_eol_index(1, &buff, buff_end).unwrap();
        let expected = 1;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_client_buffer_process() {
        let mut client = TcpClient::new("0.0.0.0".to_string());
        let chunked_buffers = create_chunked_transmission();
        for chunk in chunked_buffers.into_iter() {
            let buff_end = &chunk.iter().position(|p| p.eq(&0)).unwrap() - 1;
            &client.process_buffer(&chunk, buff_end);
        }
        println!("{}", std::str::from_utf8(&client.raw_msg_queue[0].data).unwrap());
        println!("{}", client.raw_msg_queue.len());
        assert_eq!(client.raw_msg_queue.len(), 29);
    }


}