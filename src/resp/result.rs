use std::fmt;
use std::fmt::Formatter;

#[derive(Debug)]
pub enum RespError {
    InvalidPassword(String),
    CommandNotFound(String),
    IncompleteCommand,
    NoData,
    InvalidArgument(String),
    ProtocolOutOfRange(String),
    CmdNotImplemented(String),
    FailedToSetClientInfo,
    UTFConversionError,
    FailedToConvertToUsize(String),
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
            RespError::UTFConversionError => write!(f, "Could not convert message to uf8"),
            RespError::FailedToConvertToUsize(err) => {
                write!(f, "Failed conversion to usize: {}", err)
            }
            RespError::FailedToSetClientInfo => write!(f, "Failed to set client info"),
            _ => {
                todo!()
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, RespError>;
