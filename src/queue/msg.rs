use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Message {
    pub body: Vec<u8>,
    #[serde(default = "default_message_id")]
    pub id: String,
    #[serde(default = "default_attempt")]
    pub attempt: u8,
}

impl Message {
    pub fn new(body: Vec<u8>) -> Message {
        Message {
            body,
            id: default_message_id(),
            attempt: default_attempt(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct InflightMessage {
    pub msg: Message,
    pub complete: bool,
    pub created_at: DateTime<Utc>,
}

pub fn default_attempt() -> u8 {
    1
}

pub fn default_message_id() -> String {
    Uuid::new_v4().to_string()
}
