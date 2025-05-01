use crate::resp::msg::RespMsg;
use crate::resp::result::*;
use std::str::FromStr;
use strum_macros::EnumString;

const AUTH: &str = "auth";
const RESP_MSG_DATA_TYPE_LINE: usize = 2;
const RESP_FIRST_ARG_IDX: usize = 4;
const RESP_HELLO_PROTOCOL_VERSION_INDEX: usize = 1;

#[derive(Debug, EnumString, Clone)]
pub enum CommandName {
    #[strum(ascii_case_insensitive)]
    HELLO,
    #[strum(ascii_case_insensitive)]
    LPUSH,
    #[strum(ascii_case_insensitive)]
    LPOP,
    #[strum(ascii_case_insensitive)]
    ACK,
    #[strum(ascii_case_insensitive)]
    QUEUE,
    #[strum(ascii_case_insensitive)]
    CLIENT,
}

#[derive(Debug)]
struct LPop {
    pub key: String,
    pub count: Option<u32>,
}

impl LPop {
    pub fn new(msg: RespMsg) -> Result<Self> {
        let mut count: Option<u32> = None;
        let key = msg.arg_to_str(0)?;
        if msg.args.len() >= 1 {
            count = Some(
                msg.arg_to_u32(1)
                    .map_err(|_| RespError::UTFConversionError)?,
            );
        }
        Ok(LPop { key, count })
    }
}

#[derive(Debug)]
struct LPush {
    pub key: String,
    pub elements: Vec<Vec<u8>>,
}

impl LPush {
    pub fn new(mut msg: RespMsg) -> Result<Self> {
        let key = msg.arg_to_str(0)?;
        // we don't need the key/first element so we subtract 2 instead of 1.
        let mut elements = Vec::with_capacity(msg.args.len() - 2);
        for i in 1..msg.args.len() {
            let element = std::mem::take(&mut msg.args[i]);
            msg.args.remove(i);
            elements.push(element);
        }
        Ok(LPush { key, elements })
    }
}

#[derive(Debug)]
struct Hello {
    pub user: Option<String>,
    pub password: Option<String>,
    pub protocol_version: u8,
}

impl Hello {
    pub fn new(msg: RespMsg) -> Result<Self> {
        let mut user: Option<String> = None;
        let mut password: Option<String> = None;
        let mut args = msg.args_to_str_vec()?;
        let protocol_version = args[0]
            .parse::<u8>()
            .map_err(|_| RespError::ProtocolOutOfRange(args[0].clone()))?;
        for arg in args {
            if arg == AUTH {
                user = Some(arg);
            } else if arg == AUTH {
                password = Some(arg);
            }
        }
        Ok(Hello {
            user,
            password,
            protocol_version,
        })
    }
}

#[derive(Debug)]
struct ClientSetInfo {
    pub name: Option<String>,
    pub version: Option<String>,
}

impl ClientSetInfo {
    pub fn new(msg: RespMsg) -> Result<Self> {
        let args = msg.args_to_str_vec()?;
        let mut name: Option<String> = None;
        let mut version: Option<String> = None;
        let attribute_position = 1;
        let attribute_name = &args[attribute_position];
        if attribute_name == "LIB-NAME" {
            name = Some(args[2].clone());
        } else if attribute_name == "LIB-VER" {
            version = Some(args[2].clone());
        }
        if name.is_none() && version.is_none() {
            return Err(RespError::FailedToSetClientInfo);
        }
        Ok(ClientSetInfo { name, version })
    }
}

#[derive(Debug)]
pub enum Cmd {
    LPOP(LPop),
    // LPUSH key element [element ...]
    LPUSH(LPush),
    // HELLO [protover [AUTH username password] [SETNAME clientname]]
    HELLO(Hello),
    ClientSetinfo(ClientSetInfo),
    Unknown,
}

impl Cmd {
    pub fn new(msg: RespMsg) -> Result<Self> {
        let cmd = CommandName::from_str(&msg.cmd_name)
            .map_err(|_| RespError::CommandNotFound(msg.cmd_name.to_string()))?;
        let result = match cmd {
            CommandName::LPUSH => Cmd::LPUSH(LPush::new(msg)?),
            CommandName::HELLO => Cmd::HELLO(Hello::new(msg)?),
            CommandName::CLIENT => Cmd::ClientSetinfo(ClientSetInfo::new(msg)?),
            // CommandSet::LPOP => create_lpop_cmd(msg, &msg_line_indexes),
            _ => return Err(RespError::CmdNotImplemented(msg.cmd_name.to_string())),
        };
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resp::reader::RespReader;
    use crate::test_utils::*;

    #[test]
    fn it_creates_hello() {
        let buff = serialize_to_resp(vec!["hello", "3"]);
        let resp_msg = RespReader::read_complete_buffer(&buff).unwrap();
        let cmd = Cmd::new(resp_msg).unwrap();
        match cmd {
            Cmd::HELLO(h) => {
                assert_eq!(h.user, None);
                assert_eq!(h.password, None);
                assert_eq!(h.protocol_version, 3);
            }
            _ => {
                panic!("invalid cmd");
            }
        }
    }

    #[test]
    fn when_creating_invalid_hello_it_raises_protocol_out_of_range_error() {
        let invalid_protocol_version = "a";
        let buff = serialize_to_resp(vec!["hello", invalid_protocol_version]);
        let resp_msg = RespReader::read_complete_buffer(&buff).unwrap();
        let cmd_result = Cmd::new(resp_msg);
        assert!(cmd_result.is_err());
        assert!(matches!(
            cmd_result.unwrap_err(),
            RespError::ProtocolOutOfRange(_)
        ));
    }

    #[test]
    fn it_creates_lpush() {
        let expected_args = ["value", "value2"];
        let mut cmd_key_args = vec!["lpush", "key1"];
        cmd_key_args.extend(expected_args.iter());

        let buff = serialize_to_resp(cmd_key_args);
        let resp_msg = RespReader::read_complete_buffer(&buff).unwrap();
        let cmd = Cmd::new(resp_msg).unwrap();

        match cmd {
            Cmd::LPUSH(lpush) => {
                assert_eq!(lpush.key, "key1".to_string());
                assert_eq!(lpush.elements.len(), expected_args.len());
                for i in 0..expected_args.len() {
                    let expected_value = expected_args[i];
                    let actual_value = std::str::from_utf8(&lpush.elements[i]).unwrap();
                    assert_eq!(actual_value, expected_value);
                }
            }
            _ => {
                panic!("invalid cmd");
            }
        }
    }

    #[test]
    fn it_creates_client_set_info() {
        let expected_name = "go-redis";
        let buff = serialize_to_resp(vec!["client", "setinfo", "LIB-NAME", expected_name]);
        let resp_msg = RespReader::read_complete_buffer(&buff).unwrap();
        let cmd = Cmd::new(resp_msg).unwrap();
        match cmd {
            Cmd::ClientSetinfo(client_set_info) => {
                assert_eq!(client_set_info.name.unwrap(), expected_name);
                assert_eq!(client_set_info.version, None);
            }
            _ => {
                panic!("invalid cmd");
            }
        }
    }
}
