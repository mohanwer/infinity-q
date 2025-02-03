use crate::constants::{ASCII_CARRIAGE_RETURN, ASCII_LINE_FEED};
use crate::server::SerializeError;
use std::str;

pub fn index_is_at_delimiter(index: usize, buff: &[u8]) -> bool {
    index < buff.len()
        && 2 <= index
        && buff[index] == ASCII_LINE_FEED
        && buff[index - 1] == ASCII_CARRIAGE_RETURN
}

pub fn get_eol_index(start: usize, buff: &[u8]) -> Result<usize, SerializeError> {
    let mut end = start;

    while end < buff.len() - 1 && buff[end] != 0 && !index_is_at_delimiter(end, buff) {
        end += 1;
    }

    if !index_is_at_delimiter(end, buff) {
        return Err(SerializeError::IncompleteLine);
    }

    Ok(end)
}

pub fn get_zero_byte_index(start: usize, buff: &[u8]) -> usize {
    let mut end = start;

    while end + 1 < buff.len() - 1 && buff[end + 1] > 0 {
        end += 1;
    }

    end
}

pub fn remove_empty_data(buff: &[u8]) -> &[u8] {
    let end_of_buff = get_zero_byte_index(0, buff);
    &buff[..=end_of_buff]
}

pub fn read_line(start: usize, buff: &[u8]) -> &[u8] {
    let eol_idx_result = get_eol_index(start, &buff);
    let mut read_idx_end: usize =
        eol_idx_result.unwrap_or_else(|_| get_zero_byte_index(start, &buff));
    &buff[start..=read_idx_end]
}

pub fn from_utf8_without_delimiter(buff: &[u8]) -> Result<&str, SerializeError> {
    let buff_end = buff.len() - 1;
    let buff_read_to: usize;

    buff_read_to = if index_is_at_delimiter(buff_end, buff) {
        buff_end - 2
    } else {
        buff_end
    };
    let res = str::from_utf8(&buff[..=buff_read_to]).map_err(|err| {
        println!("{}", err);
        SerializeError::UnsupportedTextEncoding
    });
    Ok(res?)
}
