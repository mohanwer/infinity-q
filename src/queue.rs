use std::cmp::{min};
use serde::{Deserialize, Serialize};
use std::collections::{VecDeque};
use chrono::{DateTime, Duration, Utc};
use uuid::{Uuid};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Message {
    #[serde(rename="messageBody")]
    body: String,
    #[serde(rename="queueUrl")]
    queue_url: String,
    #[serde(default="default_message_id")]
    id: String,
    #[serde(default="default_attempt")]
    attempt: u8
}

pub fn default_attempt() -> u8 { 1 }

pub fn default_message_id() -> String { Uuid::new_v4().to_string() }

#[derive(Clone, Debug)]
pub struct InflightMessage {
    msg: Message,
    complete: bool,
    created_at: DateTime<Utc>
}

pub struct Lifo {
    name: String,
    in_flight_expiration_ms: i64,
    queue: VecDeque<Message>,
    in_flight: VecDeque<InflightMessage>
}

impl Lifo {
    const MAX_ATTEMPT: u8 = 3;

    fn create(name: String) -> Lifo {
        Lifo {
            name,
            in_flight_expiration_ms: 1000,
            queue: VecDeque::new(),
            in_flight: VecDeque::new()
        }
    }

    fn create_with_expiration(name: String, in_flight_expiration_ms: i64) -> Lifo {
        Lifo {
            name,
            in_flight_expiration_ms,
            queue: VecDeque::new(),
            in_flight: VecDeque::new()
        }
    }

    fn message_expired(&self, msg: &InflightMessage) -> bool {
        msg.created_at + Duration::milliseconds(self.in_flight_expiration_ms) < Utc::now()
    }

    fn add(&mut self, msg: Message) {
        self.queue.push_back(msg);
    }

    fn show_in_flight(&self, cnt: usize) -> Vec<&InflightMessage> {
        let q_size = min(cnt, self.in_flight.len());
        self.in_flight.range(..q_size).into_iter().collect::<Vec<&InflightMessage>>()
    }

    fn complete(&mut self, id: &String) {
        let idx = self.in_flight.iter().position(|x| &x.msg.id == id);
        if idx.is_none() {
            return;
        }
        let i = idx.unwrap();
        let inflight_msg = self.in_flight.get_mut(i).unwrap();
        inflight_msg.complete = true;
    }

    fn sweep_in_flight(&mut self) {
        while !self.in_flight.is_empty() {
            let first_msg = self.in_flight.back().unwrap();
            if first_msg.complete {
                self.in_flight.pop_back();
            } else if self.message_expired(first_msg) {
                let mut inflight_msg = self.in_flight.pop_front().unwrap();
                if inflight_msg.msg.attempt < Self::MAX_ATTEMPT {
                    inflight_msg.msg.attempt += 1;
                    self.queue.push_front(inflight_msg.msg);
                }
            } else {
                break;
            }
        }
    }

    fn pop(&mut self, cnt: usize) -> Vec<Message> {
        let mut deque_cnt = cnt.clone();
        self.sweep_in_flight();
        let mut v = Vec::with_capacity(deque_cnt);
        while deque_cnt > 0 {
            let wrapped_msg = self.queue.pop_front();
            if wrapped_msg.is_none() {
                break;
            }
            let msg = wrapped_msg.unwrap();
            v.push(msg.clone());
            let new_msg = InflightMessage {
                msg,
                complete: false,
                created_at: Utc::now()
            };
            self.in_flight.push_back(new_msg);
            deque_cnt -= 1;
        }
        v.shrink_to_fit();
        v
    }
}

#[cfg(test)]
mod tests {
    use rand::prelude::*;
    use super::*;

    const QUEUE_NAME: &str = "a";
    const MSG_BODY: &str = "1";

    fn create_msg() -> Message {
        Message {
            body: MSG_BODY.to_string(),
            queue_url: "123".to_string(),
            id: default_message_id(),
            attempt: 1
        }
    }

    fn setup() -> Lifo {
        let mut q = Lifo::create(String::from(QUEUE_NAME));
        let msg = Message {
            body: MSG_BODY.to_string(),
            queue_url: "123".to_string(),
            id: default_message_id(),
            attempt: 1
        };
        q.add(msg);
        q
    }

    fn populate_wit_msgs(q: &mut Lifo) {
        const MSG_CNT: usize = 1000;
        for _ in 0..MSG_CNT {
            let msg = create_msg();
            q.add(msg);
        }
    }

    #[test]
    fn test_create() {
        let q = setup();
        assert_eq!(q.name, QUEUE_NAME);
    }

    #[test]
    fn test_add() {
        let q = setup();
        let loaded_msg = q.queue.back().unwrap();
        assert_eq!(loaded_msg.body, MSG_BODY);
    }

    #[test]
    fn test_one_pop() {
        let mut q = setup();
        let mut msg_q = q.pop(1);
        let popped_msg = msg_q.first_mut().unwrap();
        let inflight_msgs = q.show_in_flight(1);
        let inflight_msg = inflight_msgs.first().unwrap();
        assert_eq!(popped_msg.id, inflight_msg.msg.id);
    }

    #[test]
    fn test_many_pop() {
        const MSG_CNT: usize = 1000;
        let mut q = Lifo::create(String::from(QUEUE_NAME));
        let mut v = Vec::new();
        for _ in 0..MSG_CNT {
            let msg = create_msg();
            v.push(msg.id.clone());
            q.add(msg);
        }
        v.shrink_to_fit();
        q.pop(MSG_CNT);
        assert_eq!(q.in_flight.len(), MSG_CNT);

        let mut rng = rand::thread_rng();
        v.shuffle(&mut rng);
        for id in v.iter() {
            q.complete(id);
        }
        q.sweep_in_flight();
        assert_eq!(q.in_flight.len(), 0);
    }

    #[test]
    fn test_sweep_in_flight() {
        const MSG_CNT: usize = 1000;
        let mut q = Lifo::create_with_expiration(String::from(QUEUE_NAME), 0);
        populate_wit_msgs(&mut q);

        for _ in 1..Lifo::MAX_ATTEMPT {
            q.pop(MSG_CNT);
            q.sweep_in_flight();
            // should place all messages back in primary queue.
            assert_eq!(q.queue.len(), MSG_CNT);
        }

        q.pop(MSG_CNT);
        q.sweep_in_flight();

        // after the final attempt, the messages are dropped
        assert_eq!(q.queue.len(), 0);
        assert_eq!(q.in_flight.len(), 0);
    }

    #[test]
    fn test_show_in_flight() {
        let mut q = setup();
        let msgs = q.pop(1);
        let msg = msgs.first().unwrap();
        let v = q.show_in_flight(1);
        let in_flight_msg = &v.first().unwrap().msg;
        assert_eq!(msg.id, in_flight_msg.id);
    }
}
