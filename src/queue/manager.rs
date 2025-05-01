use crate::queue::lifo::Lifo;
use crate::resp::cmd::Cmd;
use crossbeam_channel::Receiver;
use std::collections::HashMap;

struct QueueItem {
    name: String,
    queue: Lifo,
    transmitter
}



struct QueueManager {
    queue_map: HashMap<String, Lifo>,
    cmd_rx: Receiver<Cmd>,
}

impl QueueManager {
    pub fn new(receiver: Receiver<Cmd>) -> QueueManager {
        QueueManager {
            queue_map: HashMap::new(),
            cmd_rx: receiver,
        }
    }

    pub fn create_queue
}
