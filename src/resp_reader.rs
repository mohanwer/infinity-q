use crate::constants::{ASCII_ASTERISK, RESP_BUFFER_SIZE};
use crate::server::SerializeError;
use crate::utils::{from_utf8_without_delimiter, index_is_at_delimiter};

pub type Result<T> = std::result::Result<T, SerializeError>;

#[derive(Debug, Clone)]
pub struct RespBuffer {
    data: [u8; RESP_BUFFER_SIZE],
    bytes_read: usize,
}

#[derive(Debug, Clone)]
pub struct RespReader {
    data: Vec<RespBuffer>,
    delimiters_read: u32,
    read_buffer_size: usize,
    pub expected_delimiter_cnt: u32,
    pub reached_end_of_msg: bool,
}

impl RespReader {
    pub fn new() -> Self {
        RespReader {
            data: Vec::new(),
            delimiters_read: 0,
            expected_delimiter_cnt: 0,
            read_buffer_size: RESP_BUFFER_SIZE,
            reached_end_of_msg: false,
        }
    }

    pub fn reset(&mut self) {
        self.data.clear();
        self.delimiters_read = 0;
        self.read_buffer_size = 0;
        self.expected_delimiter_cnt = 0;
        self.reached_end_of_msg = false;
    }

    pub fn try_read_size(&self, buff: &[u8]) -> Result<u32> {
        if buff.len() < 4 || buff[0] != ASCII_ASTERISK {
            return Err(SerializeError::IncompleteCommand);
        }
        let size_utf8 = from_utf8_without_delimiter(&buff[1..])?;
        let size = size_utf8
            .parse::<u32>()
            .map_err(|_| SerializeError::UnsupportedTextEncoding)?;
        // The expected command size for the array incoming is multiplied by two
        // Each array element will contain the size and then element.
        // One is added in because the first element in the array is array size.
        Ok(size * 2 + 1)
    }

    fn read_byte(&mut self, i: usize, buff: &[u8]) -> Result<bool> {
        if index_is_at_delimiter(i, buff) {
            if self.expected_delimiter_cnt == 0 {
                self.expected_delimiter_cnt = self.try_read_size(&buff[..=i])?;
            }
            self.delimiters_read += 1;
        }
        self.reached_end_of_msg =
            self.expected_delimiter_cnt != 0 && self.delimiters_read == self.expected_delimiter_cnt;
        let continue_reading = !self.reached_end_of_msg && i < buff.len();
        Ok(continue_reading)
    }

    pub fn read(
        &mut self,
        read_start: usize,
        read_end: usize,
        buff: [u8; RESP_BUFFER_SIZE],
    ) -> Result<usize> {
        let mut i = read_start;
        while self.read_byte(i, &buff[..read_end])? {
            i += 1
        }
        self.data.push(RespBuffer {
            data: buff,
            bytes_read: i,
        });
        Ok(i)
    }

    pub fn write_to_utf8(&self) -> Result<String> {
        let mut utf_data = Vec::with_capacity(self.data.len());
        for i in 0..self.data.len() {
            let resp_buffer = &self.data[i];
            utf_data[i] = String::from_utf8_lossy(&resp_buffer.data[..=resp_buffer.bytes_read]);
        }
        Ok(utf_data.join(""))
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
        let bytes_read = r.read(0, hello.len(), buff).unwrap();
        assert_eq!(bytes_read, hello.len() - 1);
    }

    #[test]
    fn test_read_chunked_transmission() {
        let mut reader = RespReader::new();
        let cmds = create_lpush_and_sadd_cmds();
        let mut buffer = convert_to_arr(&cmds);
        let bytes_read = reader.read(0, buffer.len(), buffer).unwrap();
        assert_eq!(reader.reached_end_of_msg, true);
        assert_eq!(bytes_read, 49);
    }
}
