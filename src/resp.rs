use crate::resp_buffered_reader::RespBufferedReader;
use std::fmt;
use std::fmt::Formatter;
use std::str::{FromStr, Split};
use strum_macros::EnumString;

#[derive(Debug)]
pub enum RespError {
    InvalidPassword(String),
    CommandNotFound(String),
    IncompleteCommand,
    NoData,
    InvalidArgument(String),
    ProtocolOutOfRange(String),
    CmdNotImplemented(String),
}

impl fmt::Display for RespError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            RespError::InvalidPassword(err) => write!(f, "invalid pw for {}", err),
            RespError::CommandNotFound(err) => write!(f, "invalid cmd for {}", err),
            RespError::IncompleteCommand => write!(f, "incomplete cmd"),
            RespError::InvalidArgument(err) => write!(f, "invalid arg for {}", err),
            RespError::NoData => write!(f, "no data"),
            RespError::ProtocolOutOfRange(err) => write!(f, "{} protocol out of range", err),
            RespError::CmdNotImplemented(err) => write!(f, "{} not implemented", err),
            _ => {
                todo!()
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, RespError>;

const LPUSH: [u8; 5] = [108, 112, 117, 115, 104];

#[derive(Debug, EnumString)]
enum CommandSet {
    HELLO,
    PUSH,
    ACK,
    QUEUE,
}

#[derive(Debug, EnumString)]
enum HelloKeys {
    SETNAME,
    AUTH,
    PASSWORD,
}

#[derive(Debug)]
pub enum Cmd {
    LPOP {
        key: String,
        count: u32,
    },
    LPUSH {
        key: String,
        elements: Vec<Vec<u8>>,
    },
    HELLO {
        auth: Option<String>,
        password: Option<String>,
        protocol_version: u8,
        setname: Option<String>,
    },
    SADD {
        key: String,
        member: Vec<String>,
    },
    Unknown,
}

const ADMIN: &str = "admin";
const ADMIN_PW: &str = "password";
const RESP_MSG_DATA_TYPE_LINE: usize = 2;
const RESP_LPUSH_KEY_LINE: usize = 4;

fn return_next<'a>(payload: &mut Split<'a, &str>) -> Result<&'a str> {
    match payload.next() {
        None => Err(RespError::NoData),
        Some(v) => {
            if v.starts_with("$") {
                return return_next(payload);
            }
            Ok(v)
        }
    }
}

pub fn create_line_indexes(line_breaks: &Vec<usize>) -> Vec<(usize, usize)> {
    let mut lines = Vec::with_capacity(line_breaks.len());
    for i in 1..line_breaks.len() {
        lines[i - 1] = (line_breaks[i - 1] + 1, line_breaks[i] - 2);
    }
    lines
}

pub fn read_raw_msg(mut msg: Vec<u8>, line_breaks: &Vec<usize>) -> Result<Cmd> {
    if line_breaks.len() < 3 {
        return Err(RespError::IncompleteCommand);
    }
    let arg_count = line_breaks.len();
    let msg_line_indexes = create_line_indexes(line_breaks);
    let (read_start, read_end) = msg_line_indexes[RESP_MSG_DATA_TYPE_LINE];
    let msg_type = std::str::from_utf8(&msg[read_start..=read_end])
        .map_err(|_| RespError::InvalidArgument("invalid message type".into()))?;
    if msg_type == "LPUSH" {
        let mut elements = Vec::with_capacity(line_breaks.len());
        let (key_start, key_end) = msg_line_indexes[RESP_LPUSH_KEY_LINE];
        let key = String::from_utf8(msg[key_start..=key_end].to_vec());
        for i in (RESP_LPUSH_KEY_LINE + 2..arg_count).step(2) {
            let (read_start, read_end) = msg_line_indexes[i];
            elements.push(msg[read_start..=read_end].to_vec());
        }
        return Ok(Cmd::LPUSH { key, elements });
    }
}

pub fn read_raw_cmd(raw_cmd: RespBufferedReader) -> Result<Cmd> {
    let cmd_utf8 = raw_cmd.write_to_utf8().unwrap();
    let mut it = cmd_utf8.split("\r\n");
    map_command(&mut it)
}

pub fn map_command(payload: &mut Split<&str>) -> Result<Cmd> {
    let first_word = return_next(payload)?;
    let type_of_cmd_result = CommandSet::from_str(&first_word);
    let Ok(type_of_cmd) = type_of_cmd_result else {
        return Err(RespError::CommandNotFound(first_word.to_string()));
    };
    match type_of_cmd {
        CommandSet::HELLO => deserialize_auth(payload),
        CommandSet::QUEUE | CommandSet::ACK | CommandSet::PUSH => {
            Err(RespError::CmdNotImplemented(first_word.to_string()))
        }
    }
}

fn get_protocol_version(payload: &mut Split<&str>) -> Result<u8> {
    let raw_next = return_next(payload)?;

    let protocol_version_result = raw_next.parse::<u8>();
    match protocol_version_result {
        Ok(protocol_version) => Ok(protocol_version),
        Err(_) => Err(RespError::ProtocolOutOfRange(raw_next.to_string())),
    }
}

fn deserialize_auth(payload: &mut Split<&str>) -> Result<Cmd> {
    let protocol_version = get_protocol_version(payload)?;
    let mut auth: Option<String> = None;
    let mut password: Option<String> = None;
    let mut setname: Option<String> = None;
    while let (Some(key), Some(value)) = (payload.next(), payload.next()) {
        let valid_key = HelloKeys::from_str(key);
        match valid_key {
            Ok(hello_key) => match hello_key {
                HelloKeys::AUTH => {
                    auth = Some(value.to_string());
                }
                HelloKeys::SETNAME => {
                    setname = Some(value.to_string());
                }
                HelloKeys::PASSWORD => {
                    password = Some(value.to_string());
                }
            },
            Err(_) => return Err(RespError::InvalidArgument(value.to_string())),
        }
    }

    Ok(Cmd::HELLO {
        auth,
        password,
        protocol_version,
        setname,
    })
}
