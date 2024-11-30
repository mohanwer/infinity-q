use crate::constants::{ASCII_ASTERISK, ASCII_CARRIAGE_RETURN, ASCII_LINE_FEED};
use crate::server::SerializeError;
use crate::utils::{get_eol_index, is_end_of_line_index};
use std::{str};

#[derive(Default)]
pub struct RawCmd {
    pub(crate) data: Vec<u8>,
    pub(crate) eol_exists: bool,
    size: Option<usize>,
    last_read_idx: usize,
    cr_nr_count: usize,
    pub(crate) complete: bool
}

impl RawCmd {
    pub fn new() -> RawCmd {
        RawCmd {
            data: Vec::with_capacity(1024),
            eol_exists: false,
            ..Default::default()
        }
    }

    pub fn from_vec(data: Vec<u8>) -> RawCmd {
        RawCmd {
            data,
            eol_exists: true,
            ..Default::default()
        }
    }

    pub fn size(&mut self) -> Result<usize, SerializeError> {
        let first_line_result = self.first_line_eol();
        if first_line_result.is_err() {
            return Err(first_line_result.err().unwrap())
        }
        match first_line_result {
            Ok(eol) => {
                let first_line_result = str::from_utf8(&self.data[1..=eol]);
                match first_line_result {
                    Ok(first_line) => {
                        // Add 1 to account for the first line
                        // The first resp line has array size. We are assuming we are always
                        // passing in the size of each key and then value of the key resulting in 2x
                        // the size of the array.
                        let command_size_result = first_line.parse::<usize>();
                        if command_size_result.is_err() {
                            Err(SerializeError::UnreadableCommandSize)
                        } else {
                            let mut cmd_size = command_size_result
                                .expect("Unable to parse command size");
                            let cmd_size_with_attr_lengths = cmd_size * 2 + 1;
                            self.size = Some(cmd_size_with_attr_lengths);
                            Ok(cmd_size_with_attr_lengths)
                        }
                    },
                    Err(err) => { Err(SerializeError::UnsupportedTextEncoding(err)) }
                }
            }
            Err(err) => { Err(err) }
        }
    }

    pub fn is_last_line_complete(&self) -> bool {
        if self.data.len() < 4 { return false }
        let n = self.data.len() - 1;
        self.data[n-1] == ASCII_CARRIAGE_RETURN && self.data[n] == ASCII_LINE_FEED
    }

    pub fn first_line_eol(&self) -> Result<usize, SerializeError> {
        if self.data.len() < 4 || self.data[0] != ASCII_ASTERISK
        {
            return Err(SerializeError::IncompleteCommand)
        }
        let first_eol_result = get_eol_index(0, &self.data, self.data.len());
        match first_eol_result {
            Ok(eol) => { Ok(eol - 2) /* strip line feed & carriage return */ }
            Err(_) => { Err(SerializeError::IncompleteLine) }
        }
    }

    pub fn all_lines_received(&mut self) -> Result<bool, SerializeError> {
        let data_size_result = self.size();
        if data_size_result.is_err() {
            let eol_result = self.first_line_eol().err().expect("Unable to error");
            return Err(eol_result);
        }
        let data_size = data_size_result?;
        while self.last_read_idx + 1 < self.data.len() && self.cr_nr_count < data_size {
            self.last_read_idx += 1;
            if is_end_of_line_index(self.last_read_idx, &self.data) {
                self.cr_nr_count += 1;
            }
        }
        self.complete = self.cr_nr_count == data_size;
        Ok(self.complete)
    }

    pub fn extend(&mut self, line: &Vec<u8>) -> Result<bool, SerializeError> {
        self.data.extend(line);
        match self.all_lines_received() {
            Ok(msg_complete) => {Ok(msg_complete) }
            Err(err) => {Err(err)}
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::raw_cmd::RawCmd;

    fn create_incomplete_first_line() -> Vec<u8> {
        vec![42,53]
    }

    fn create_hello_cmd() -> RawCmd {
        RawCmd::from_vec(
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
        )
    }

    fn create_partial_hello_cmd() -> RawCmd {
        RawCmd::from_vec(
            vec![
                42,53,13,10,
                36,53
            ]
        )
    }

    #[test]
    fn test_is_last_line_complete_returns_false_without_data() {
        let cmd = RawCmd::new();
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
        assert_eq!(1, cmd.cr_nr_count);
    }

}