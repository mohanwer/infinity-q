use crate::constants::{ASCII_ASTERISK, ASCII_CARRIAGE_RETURN, ASCII_LINE_FEED};
use crate::server::SerializeError;
use crate::utils::{from_utf8_without_delimiter, get_eol_index, index_is_at_delimiter, read_line};

const DEFAULT_CMD_CAPACITY: usize = 1024;
pub type Result<T> = std::result::Result<T, SerializeError>;

#[derive(Debug)]
pub struct RespBuffReadResult {
    pub(crate) end_of_message_reached: bool,
    pub(crate) bytes_read: usize,
}

impl RespBuffReadResult {
    pub fn new() -> Self {
        RespBuffReadResult {
            end_of_message_reached: false,
            bytes_read: 0,
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct RespBufferedReader {
    pub(crate) data: Vec<u8>,
    pub(crate) eol_exists: bool,
    size: Option<usize>,
    last_read_idx: usize,
    delimiter_cnt: usize,
    pub(crate) reached_end_of_msg: bool,
    read_index: usize,
}

impl RespBufferedReader {
    pub fn new() -> RespBufferedReader {
        RespBufferedReader {
            data: Vec::with_capacity(DEFAULT_CMD_CAPACITY),
            eol_exists: false,
            ..Default::default()
        }
    }

    pub fn from_vec(data: Vec<u8>) -> RespBufferedReader {
        let mut cmd = RespBufferedReader {
            data: Vec::with_capacity(DEFAULT_CMD_CAPACITY),
            eol_exists: true,
            ..Default::default()
        };
        let _ = cmd.extend(data.as_slice());
        cmd
    }

    pub fn reset(&mut self) {
        self.data.clear();
        self.eol_exists = false;
        self.size = None;
        self.last_read_idx = 0;
        self.delimiter_cnt = 0;
        self.reached_end_of_msg = false;
        self.read_index = 0;
    }

    pub fn size(&mut self) -> Result<usize> {
        let first_line_result = self.first_line_eol();
        match first_line_result {
            Ok(eol) => {
                let first_line_result = from_utf8_without_delimiter(&self.data[1..=eol]);
                match first_line_result {
                    Ok(first_line) => {
                        let command_size_result = first_line.parse::<usize>();
                        if command_size_result.is_err() {
                            Err(SerializeError::UnreadableCommandSize)
                        } else {
                            let cmd_size =
                                command_size_result.expect("Unable to parse command size");
                            let cmd_size_with_attr_lengths = cmd_size * 2 + 1;
                            self.size = Some(cmd_size_with_attr_lengths);
                            Ok(cmd_size_with_attr_lengths)
                        }
                    }
                    Err(_) => Err(SerializeError::UnsupportedTextEncoding),
                }
            }
            Err(err) => Err(err),
        }
    }

    pub fn is_last_line_complete(&self) -> bool {
        if self.data.len() < 4 {
            return false;
        }
        let mut n = self.data.len() - 1;
        self.data[n - 1] == ASCII_CARRIAGE_RETURN && self.data[n] == ASCII_LINE_FEED
    }

    pub fn first_line_eol(&self) -> Result<usize> {
        if self.data.len() < 4 || self.data[0] != ASCII_ASTERISK {
            return Err(SerializeError::IncompleteCommand);
        }
        get_eol_index(0, &self.data)
    }

    pub fn all_lines_received(&mut self) -> Result<bool> {
        let expected_delimiter_cnt = self.size()?;
        while self.last_read_idx + 1 < self.data.len()
            && self.delimiter_cnt < expected_delimiter_cnt
        {
            self.last_read_idx += 1;
            if index_is_at_delimiter(self.last_read_idx, &self.data) {
                self.delimiter_cnt += 1;
            }
        }
        self.reached_end_of_msg = self.delimiter_cnt == expected_delimiter_cnt;
        Ok(self.reached_end_of_msg)
    }

    pub fn extend(&mut self, buff: &[u8]) -> Result<bool> {
        self.data.extend(buff);
        Ok(self.all_lines_received()?)
    }

    pub fn read(&mut self, buff: &[u8]) -> Result<usize> {
        let mut read_cursor: usize = 0;
        while read_cursor < buff.len() {
            let line = read_line(read_cursor, buff);
            read_cursor += line.len();
            let msg_processing_result = self.extend(&line);
            match msg_processing_result {
                Err(err) => match &err {
                    SerializeError::IncompleteLine
                    | SerializeError::MissingContentSize
                    | SerializeError::IncompleteCommand
                    | SerializeError::UnreadableCommandSize => continue,
                    SerializeError::UnsupportedTextEncoding => {
                        return Err(SerializeError::UnsupportedTextEncoding);
                    }
                },
                Ok(command_transmission_complete) => {
                    if command_transmission_complete {
                        return Ok(read_cursor);
                    }
                }
            }
        }

        Ok(read_cursor)
    }
    pub fn write_to_utf8(&self) -> Result<String> {
        String::from_utf8(self.data.clone()).map_err(|_| SerializeError::UnsupportedTextEncoding)
    }
}

#[cfg(test)]
mod tests {
    use crate::resp_buffered_reader::RespBufferedReader;

    fn create_incomplete_first_line() -> Vec<u8> {
        vec![42, 53]
    }

    fn create_hello_cmd() -> RespBufferedReader {
        RespBufferedReader::from_vec(vec![
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
        ])
    }

    fn create_partial_hello_cmd() -> RespBufferedReader {
        RespBufferedReader::from_vec(vec![42, 53, 13, 10, 36, 53])
    }

    #[test]
    fn test_is_last_line_complete_returns_false_without_data() {
        let cmd = RespBufferedReader::new();
        assert_eq!(false, cmd.is_last_line_complete());
    }

    #[test]
    fn test_is_last_line_complete_returns_true_with_data() {
        let cmd = create_hello_cmd();
        assert_eq!(true, cmd.is_last_line_complete());
    }

    #[test]
    fn test_first_line_eol_returns_correct_index() {
        let cmd = create_hello_cmd();
        assert_eq!(3, cmd.first_line_eol().unwrap());
    }

    #[test]
    fn test_size_returns_correct_size() {
        let mut cmd = create_hello_cmd();
        assert_eq!(11, cmd.size().unwrap());
        assert_eq!(Some(11), cmd.size);
    }

    #[test]
    fn test_message_ready_returns_true() {
        let mut cmd = create_hello_cmd();
        assert_eq!(true, cmd.all_lines_received().unwrap());
    }

    #[test]
    fn test_message_ready_returns_false() {
        let mut cmd = create_partial_hello_cmd();
        assert_eq!(false, cmd.all_lines_received().unwrap());
        assert_eq!(5, cmd.last_read_idx);
        assert_eq!(1, cmd.delimiter_cnt);
    }
}
