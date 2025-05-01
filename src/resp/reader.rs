use crate::constants::{
    ASCII_ASTERISK, ASCII_BULK_STRING, ASCII_CARRIAGE_RETURN, ASCII_LINE_FEED, RESP_BUFFER_SIZE,
    RESP_COMMAND_ARG_SIZE,
};
use crate::resp::msg::*;
use crate::resp::result::RespError::{FailedToConvertToUsize, UTFConversionError};
use crate::resp::result::Result;
use std::str;

#[derive(Debug, Clone)]
pub struct RespBuffer {
    data: [u8; RESP_BUFFER_SIZE],
    bytes_read: usize,
}

#[derive(Debug, Clone)]
pub struct RespReader {
    data: Vec<u8>,
    lines: Vec<Vec<u8>>,
    arg_indices: Vec<ArgIndex>,
    size_set: bool,
    delimiters_read: u32,
    pub expected_delimiter_cnt: u32,
    pub reached_end_of_msg: bool,
}

#[derive(Debug, Clone)]
pub struct ArgIndex {
    pub start: usize,
    pub end: usize,
}

impl RespReader {
    pub fn new() -> Self {
        let mut lines = Vec::with_capacity(10);
        lines.push(Vec::with_capacity(10));

        RespReader {
            data: Vec::with_capacity(RESP_BUFFER_SIZE),
            arg_indices: Vec::with_capacity(RESP_COMMAND_ARG_SIZE),
            lines,
            size_set: false,
            delimiters_read: 0,
            expected_delimiter_cnt: 0,
            reached_end_of_msg: false,
        }
    }

    pub fn reset(&mut self) -> Result<RespMsg> {
        let lines = std::mem::take(&mut self.lines);
        self.arg_indices.clear();
        self.lines.clear();
        self.size_set = false;
        self.delimiters_read = 0;
        self.expected_delimiter_cnt = 0;
        self.reached_end_of_msg = false;
        self.data = Vec::with_capacity(RESP_BUFFER_SIZE);
        RespMsg::new(lines)
    }

    pub fn read(&mut self, buff: &[u8]) -> Result<usize> {
        let mut i = 0;
        while i < buff.len() && !self.reached_end_of_msg {
            self.data.push(buff[i]);
            self.write_byte_unless_eol(buff[i]);
            if buff[i] == ASCII_LINE_FEED {
                if self.arg_indices.len() == 0 {
                    self.arg_indices.push(ArgIndex {
                        start: 0,
                        end: self.data.len() - 1 - 2,
                    });
                } else {
                    let last_arg_end = self.arg_indices[self.arg_indices.len() - 1].end;
                    self.arg_indices.push(ArgIndex {
                        start: last_arg_end + 3,
                        end: self.data.len() - 1 - 2,
                    });
                }
                self.delimiters_read += 1;
                if !self.size_set {
                    let size_arg_end = i - 2;
                    let size_arg_start = 1;
                    let size_utf8 = str::from_utf8(&self.data[size_arg_start..=size_arg_end])
                        .map_err(|err| UTFConversionError)?;
                    let size = size_utf8.parse::<u32>().map_err(|err| UTFConversionError)?;
                    self.size_set = true;
                    self.expected_delimiter_cnt = size * 2 + 1;
                } else {
                    self.reached_end_of_msg = self.delimiters_read == self.expected_delimiter_cnt
                }

                if !self.reached_end_of_msg {
                    self.add_empty_line()?;
                }
            }

            i += 1;
        }

        Ok(i - 1)
    }

    fn add_empty_line(&mut self) -> Result<()> {
        let mut next_line_size: usize;
        let first_byte = self.lines[self.lines.len() - 1][0];
        if first_byte == ASCII_BULK_STRING {
            // we add 2 for the delimiters
            next_line_size = self.read_next_line_size()? + 2;
            // we don't need the size anymore
            self.lines.pop();
        } else if first_byte == ASCII_ASTERISK {
            self.lines.pop();
            next_line_size = RESP_COMMAND_ARG_SIZE
        } else {
            next_line_size = RESP_COMMAND_ARG_SIZE
        }
        self.lines.push(Vec::with_capacity(next_line_size));
        Ok(())
    }

    pub fn write_byte_unless_eol(&mut self, byte: u8) {
        if byte != ASCII_LINE_FEED && byte != ASCII_CARRIAGE_RETURN {
            let last_line_index = self.lines.len() - 1;
            self.lines[last_line_index].push(byte);
        }
    }

    pub fn write_to_utf8(&self) -> Result<&str> {
        let msg = str::from_utf8(&self.data).map_err(|_| UTFConversionError)?;
        Ok(msg)
    }

    pub fn read_next_line_size(&self) -> Result<usize> {
        let raw_line = self.lines.last().unwrap();
        let raw_line_size = raw_line[1..raw_line.len()].to_vec();
        let line = String::from_utf8(raw_line_size).map_err(|_| UTFConversionError)?;
        let line_size = line
            .parse::<usize>()
            .map_err(|_| FailedToConvertToUsize(line))?;
        Ok(line_size)
    }

    pub fn read_complete_buffer(buff: &[u8]) -> Result<RespMsg> {
        let mut reader = RespReader::new();
        reader.read(buff)?;
        let msg = reader.reset()?;
        Ok(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;

    #[test]
    fn it_returns_correct_data_and_arg_count() {
        let expected_first_arg: u32 = 3;
        let expected_hello_arg_count = 1;

        let hello = serialize_to_resp(vec!["hello", "3"]);
        let buff = convert_to_arr(&hello);
        let msg = RespReader::read_complete_buffer(&hello).unwrap();
        let first_arg = msg.arg_to_u32(0).unwrap();
        assert_eq!(first_arg, expected_first_arg);
        assert_eq!(msg.args.len(), expected_hello_arg_count);
    }

    #[test]
    fn it_returns_correct_bytes_read_for_chunked_transmissions() {
        let expected_bytes_read = 49;

        let mut reader = RespReader::new();
        let cmds = create_lpush_and_sadd_cmds();
        let buffer = convert_to_arr(&cmds);
        let bytes_read = reader.read(&buffer).unwrap();

        assert_eq!(reader.reached_end_of_msg, true);
        assert_eq!(bytes_read, expected_bytes_read);
    }
}
