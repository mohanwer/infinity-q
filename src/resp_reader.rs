use crate::constants::{ASCII_LINE_FEED, RESP_BUFFER_SIZE, RESP_COMMAND_ARG_SIZE};
use crate::server::SerializeError;
use std::str;

pub type Result<T> = std::result::Result<T, SerializeError>;

#[derive(Debug, Clone)]
pub struct RespBuffer {
    data: [u8; RESP_BUFFER_SIZE],
    bytes_read: usize,
}

#[derive(Debug, Clone)]
pub struct RespReader {
    data: Vec<u8>,
    data_line_break_positions: Vec<usize>,
    data_read: usize,
    size_set: bool,
    delimiters_read: u32,
    pub expected_delimiter_cnt: u32,
    pub reached_end_of_msg: bool,
}

impl RespReader {
    pub fn new() -> Self {
        RespReader {
            data: Vec::with_capacity(RESP_BUFFER_SIZE),
            data_line_break_positions: Vec::with_capacity(RESP_COMMAND_ARG_SIZE),
            data_read: 0,
            size_set: false,
            delimiters_read: 0,
            expected_delimiter_cnt: 0,
            reached_end_of_msg: false,
        }
    }

    pub fn reset(&mut self) {
        self.data.clear();
        self.data_line_break_positions.clear();
        self.size_set = false;
        self.delimiters_read = 0;
        self.expected_delimiter_cnt = 0;
        self.reached_end_of_msg = false;
    }

    pub fn read(&mut self, buff: &[u8]) -> Result<usize> {
        let mut i = 0;
        while i < buff.len() && !self.reached_end_of_msg {
            self.data.push(buff[i]);
            if buff[i] == ASCII_LINE_FEED {
                self.data_line_break_positions.push(self.data.len());
                self.delimiters_read += 1;

                if !self.size_set {
                    let size_arg_end = i - 2;
                    let size_arg_start = 1;
                    let size_utf8 = str::from_utf8(&self.data[size_arg_start..=size_arg_end])
                        .map_err(|err| {
                            println!("{}", err);
                            SerializeError::UnsupportedTextEncoding
                        })?;
                    let size = size_utf8.parse::<u32>().map_err(|err| {
                        println!("{}", err);
                        SerializeError::UnsupportedTextEncoding
                    })?;
                    self.size_set = true;
                    self.expected_delimiter_cnt = size * 2 + 1;
                } else {
                    self.reached_end_of_msg = self.delimiters_read == self.expected_delimiter_cnt
                }
            }

            i += 1;
        }

        Ok(i - 1)
    }

    pub fn write_to_utf8(&self) -> Result<String> {
        let msg = String::from_utf8(self.data.clone())
            .map_err(|_| SerializeError::UnsupportedTextEncoding)?;
        Ok(msg)
    }
}

#[cfg(test)]
mod tests {
    use crate::resp_reader::RespReader;
    use crate::test_utils::*;

    #[test]
    fn test_read() {
        let hello = create_hello();
        let buff = convert_to_arr(&hello);
        let mut r = RespReader::new();
        let bytes_read = r.read(&buff).unwrap();
        assert_eq!(bytes_read, hello.len() - 1);
    }

    #[test]
    fn test_read_chunked_transmission() {
        let mut reader = RespReader::new();
        let cmds = create_lpush_and_sadd_cmds();
        let buffer = convert_to_arr(&cmds);
        let bytes_read = reader.read(&buffer).unwrap();
        assert_eq!(reader.reached_end_of_msg, true);
        assert_eq!(bytes_read, 49);
    }
}
