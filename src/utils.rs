use crate::constants::{ASCII_CARRIAGE_RETURN, ASCII_LINE_FEED};
use crate::server::SerializeError;

pub fn is_end_of_line_index(index: usize, buff: &Vec<u8>) -> bool {
    index < buff.len() &&
        2 <= index &&
        buff[index] == ASCII_LINE_FEED &&
        buff[index-1] == ASCII_CARRIAGE_RETURN
}

pub fn get_next_line_idx(eol_index: usize, buff: &Vec<u8>) -> usize {
    let mut end = eol_index;
    while end < buff.len() && (buff[end] == ASCII_LINE_FEED || buff[end] == ASCII_CARRIAGE_RETURN) {
        end += 1;
    }
    end
}

pub fn get_eol_index(start: usize, buff: &Vec<u8>, buffer_end: usize) -> Result<usize, SerializeError> {
    let mut end = start;

    while end <= buffer_end && !is_end_of_line_index(end, buff)  {
        end += 1;
    }

    if !is_end_of_line_index(end, buff) {
        return Err(SerializeError::IncompleteLine)
    }

    Ok(end)
}