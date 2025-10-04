use crate::constants::RESP_BUFFER_SIZE;
use crate::queue::manager::QueueManager;
use crate::resp::msg::RespMsg;
use crate::resp::reader::RespReader;
use crate::resp::result::RespError;
use std::collections::VecDeque;
use std::fmt;
use std::fmt::Formatter;
use std::string::FromUtf8Error;
use std::sync::Arc;
use tokio::io::{AsyncWriteExt, Error};
use tokio::net::TcpListener;

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
    raw_msg_queue: VecDeque<RespMsg>,
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
        read_end: usize,
    ) -> Result<(), RespError> {
        let mut read_start = 0;
        while read_start < read_end {
            read_start += self.resp_buff_reader.read(&buff[read_start..=read_end])? + 1;
            if self.resp_buff_reader.reached_end_of_msg {
                let msg = self.resp_buff_reader.reset()?;
                self.msg_from_client += 1;
                self.raw_msg_queue.push_back(msg);
            }
        }
        Ok(())
    }

    // pub fn read_msg_queue(&mut self) -> Result<>
}

pub struct TcpServer {
    q_manager: QueueManager,
}

impl TcpServer {
    pub fn new() -> TcpServer {
        TcpServer {
            q_manager: QueueManager::new(),
        }
    }

    pub async fn start(self) -> Result<(), Error> {
        let q = Arc::new(self.q_manager);
        let p = q.clone();
        p.listen_for_commands();
        let listener = TcpListener::bind("127.0.0.1:6379").await?;

        loop {
            let (socket, _) = listener.accept().await?;
            let new_q = q.clone();
            tokio::spawn(async move { new_q.handle_new_stream(socket).await });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::server::TcpClient;
    use crate::test_utils::*;
    use crate::utils::get_eol_index;

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
        for chunk in chunked_buffers.into_iter() {
            let buff = convert_to_arr(&chunk);
            client.read_buff(buff, chunk.len() - 1).unwrap();
        }
        let expected: u32 = 3;
        assert_eq!(client.msg_from_client, expected);
    }
}
