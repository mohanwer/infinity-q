use crate::constants::{ASCII_CARRIAGE_RETURN, ASCII_LINE_FEED};
use crate::server::SerializeError;
use crate::utils::get_eol_index;
use std::{str};
use std::str::Utf8Error;

pub struct RawCmd {
    pub(crate) data: Vec<u8>,
    pub(crate) eol_exists: bool,
}

impl RawCmd {
    pub fn new() -> RawCmd {
        RawCmd {
            data: Vec::new(),
            eol_exists: false,
        }
    }

    pub fn size(&self) -> Result<u8, SerializeError> {
        let first_line_result = self.first_line_eol();
        if first_line_result.is_err() {
            return Err(first_line_result.err().unwrap())
        }
        match first_line_result {
            Ok(eol) => {
                let first_line_result = str::from_utf8(&self.data[1..=eol].to_vec());
                match first_line_result {
                    Ok(first_line) => {
                        // Add 1 to account for the first line
                        // The first resp line has array size. We are assuming we are always
                        // passing in the size of each key and then value of the key resulting in 2x
                        // the size of the array.
                        let command_size_result = first_line.parse::<u8>();
                        if command_size_result.is_err() {
                            Err(SerializeError::UnreadableCommandSize)
                        } else {
                            Ok(command_size_result.unwrap() * 2 + 1)
                        }
                    },
                    Err(err) => { Err(SerializeError::UnsupportedTextEncoding(err)) }
                }
            }
            Err(err) => { Err(err) }
        }
    }

    pub fn is_last_line_complete(&self) -> bool {
        if !self.data.is_empty() { return false }
        if self.data.len() < 4 { return false }
        self.data[-1] == ASCII_CARRIAGE_RETURN && self.data[-2] == ASCII_LINE_FEED
    }

    pub fn first_line_eol(&self) -> Result<u8, SerializeError> {
        if (
            !self.data.is_empty() ||
            self.data.len() < 4 ||
            self.data[0] != ASCII_LINE_FEED
        ) {
            return Err(SerializeError::IncompleteCommand)
        }
        let first_eol_result = get_eol_index(0, &self.data, self.data.len());
        match first_eol_result {
            Ok(eol) => { Ok(eol as u8) }
            Err(_) => { Err(SerializeError::IncompleteLine) }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::raw_cmd::RawCmd;

    fn create_incomplete_first_line() -> Vec<u8> {
        vec![42,53]
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

    #[test]
    fn test_is_last_line_complete_without_data() {
        let cmd = RawCmd::new();
        assert_eq!(false, cmd.is_last_line_complete());
    }


}