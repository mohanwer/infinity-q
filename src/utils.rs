use crate::constants::{ASCII_CARRIAGE_RETURN, ASCII_LINE_FEED};
use crate::server::SerializeError;

pub fn is_end_of_line_index(index: usize, buff: &[u8]) -> bool {
    index < buff.len() &&
        2 <= index &&
        buff[index] == ASCII_LINE_FEED &&
        buff[index-1] == ASCII_CARRIAGE_RETURN
}

pub fn get_eol_index(start: usize, buff: &[u8]) -> Result<usize, SerializeError> {
    let mut end = start;

    while end <= buff.len() && buff[end] != 0 && !is_end_of_line_index(end, buff)  {
        end += 1;
    }

    if !is_end_of_line_index(end, buff) {
        return Err(SerializeError::IncompleteLine)
    }

    Ok(end)
}

pub fn read_line(start: usize, buff: &[u8]) -> Vec<u8> {
    let eol_idx_result = get_eol_index(start, &buff);
    let mut read_idx_end: usize = eol_idx_result.unwrap_or_else(|_| buff.len());
    buff[start..=read_idx_end].to_vec()
}