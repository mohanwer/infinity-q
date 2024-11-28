use std::cmp::PartialEq;
use std::str::FromStr;
use strum_macros::EnumString;
use std::fmt;
use std::fmt::Formatter;

#[derive(Debug)]
pub enum RespError {
    InvalidPassword(String),
    CommandNotFound(String),
}

impl fmt::Display for RespError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            RespError::InvalidPassword(err) => write!(f, "invalid pw for {}", err),
            RespError::CommandNotFound(err) => write!(f, "invalid cmd for {}", err),
            _ => {todo!()}
        }
    }
}

impl std::error::Error for RespError {}

#[derive(Debug, EnumString)]
enum CommandSet {
    HELLO,
    PUSH,
    ACK,
    QUEUE
}

struct AuthAttempt {
    user_name: String,
    password: String,
    protocol_version: u8
}

const ADMIN: &str = "admin";
const ADMIN_PW: &str = "password";

fn map_command(payload: String) -> Result<(), RespError>  {
    let payload_split: Vec<&str> = payload.split(' ').collect();
    let first_word: String = payload_split[0].to_string();
    let cmd_result = CommandSet::from_str(&first_word);
    if cmd_result.is_err() {
        eprintln!("{}", cmd_result.err().unwrap());
        return Err(RespError::CommandNotFound(first_word));
    }
    let cmd = cmd_result.unwrap();
    match cmd {
        CommandSet::HELLO => {
            let auth_attempt = deserialize_auth(payload_split);
            if !authorize(&auth_attempt) {
                return Err(RespError::InvalidPassword(auth_attempt.user_name))
            }
        },
        _ => return Err(RespError::CommandNotFound(first_word.to_string()))
    }

    Ok(())
}

fn authorize(auth_attempt: &AuthAttempt) -> bool {
    auth_attempt.user_name.eq(ADMIN) && auth_attempt.password.eq(ADMIN_PW)
}

fn deserialize_auth(payload: Vec<&str>) -> AuthAttempt {
    AuthAttempt {
        user_name: payload[3].to_string(),
        password: payload[4].to_string(),
        protocol_version: payload[1].parse::<u8>().unwrap()
    }
}
