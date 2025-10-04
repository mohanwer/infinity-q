use crate::constants::RESP_BUFFER_SIZE;
use crate::queue::lifo::Lifo;
use crate::queue::msg::Message;
use crate::resp::cmd::{Cmd, Hello, LPop, LPush};
use crate::resp::msg::RespMsg;
use crate::resp::reader::RespReader;
use crate::resp::result::RespError;
use crossbeam_channel::{Receiver, Sender};
use dashmap::DashMap;
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::thread;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

struct Response {
    data: Vec<u8>,
    client_id: Uuid,
    data_id: Option<String>,
}

impl Response {
    pub fn from_usize(id: Uuid, data: usize) -> Response {
        Response {
            data: vec![0; data],
            client_id: id,
            data_id: None,
        }
    }

    pub fn from_str(id: Uuid, data: String) -> Response {
        Response {
            data: data.into_bytes(),
            client_id: id,
            data_id: None,
        }
    }

    pub fn from_msg(id: Uuid, msg: Message) -> Response {
        Response {
            data: msg.body,
            client_id: id,
            data_id: Some(msg.id),
        }
    }
}

#[derive(Debug)]
struct TcpClient {
    name: String,
    id: Uuid,
    address: String,
    version: u8,
    authenticated: bool,
    msg_from_client: u32,
    msg_cnt_to_client: u32,
    connected: RwLock<bool>,
    resp_buff_reader: RespReader,
    raw_msg_queue: VecDeque<RespMsg>,
}

impl TcpClient {
    pub fn new(address: String, id: Uuid) -> TcpClient {
        TcpClient {
            id,
            name: "unknown".to_string(),
            version: 0,
            address,
            authenticated: false,
            msg_from_client: 0,
            msg_cnt_to_client: 0,
            connected: RwLock::new(false),
            resp_buff_reader: RespReader::new(),
            raw_msg_queue: VecDeque::new(),
        }
    }

    pub fn read_buff(
        &mut self,
        buff: &mut [u8; RESP_BUFFER_SIZE],
        read_end: usize,
    ) -> Result<(), RespError> {
        let mut read_start = 0;
        while read_start < read_end {
            read_start += self.resp_buff_reader.read(&buff[read_start..=read_end])? + 1;
            if self.resp_buff_reader.reached_end_of_msg {
                let msg = self.resp_buff_reader.reset()?;
                self.msg_from_client += 1;
                self.raw_msg_queue.push_back(msg);
            }
        }
        Ok(())
    }

    pub fn authenticate(&mut self, name: Option<String>, version: Option<u8>) {
        if !name.is_none() {
            self.name = name.expect("Name should not be None");
            self.authenticated = true;
        }
        if !version.is_none() {
            self.version = version.expect("Version should not be None");
        }
    }
}

pub struct QueueManager {
    queue_map: Arc<DashMap<String, Lifo>>,
    connections: DashMap<Uuid, TcpClient>,
    writer_assignments: Arc<DashMap<Uuid, VecDeque<Response>>>,
    cmd_rx: Receiver<(Cmd, Uuid)>,
    readers: DashMap<Uuid, OwnedReadHalf>,
    writers: DashMap<Uuid, OwnedWriteHalf>,
    cmd_trx: Sender<(Cmd, Uuid)>,
    write_handles: DashMap<Uuid, tokio::task::AbortHandle>,
    read_handles: DashMap<Uuid, tokio::task::AbortHandle>,
    cancellation_tokens: DashMap<Uuid, CancellationToken>,
}

impl QueueManager {
    pub fn new() -> QueueManager {
        let (sender, receiver) = crossbeam_channel::unbounded();
        QueueManager {
            queue_map: Arc::new(DashMap::new()),
            cmd_rx: receiver,
            cmd_trx: sender,
            connections: DashMap::new(),
            writer_assignments: Arc::new(DashMap::new()),
            readers: DashMap::new(),
            writers: DashMap::new(),
            write_handles: DashMap::new(),
            read_handles: DashMap::new(),
            cancellation_tokens: DashMap::new(),
        }
    }

    pub fn listen_for_commands(self: Arc<Self>) {
        let queue_map = Arc::clone(&self.queue_map);
        thread::spawn(move || loop {
            match self.cmd_rx.recv() {
                Ok((cmd, client_id)) => match cmd {
                    Cmd::LPUSH(c) => self.handle_lpush(c, client_id, &queue_map),
                    Cmd::LPOP(c) => self.handle_lpop(c, client_id, &queue_map),
                    Cmd::HELLO(c) => {}
                    _ => println!("unknown command"),
                },
                Err(e) => println!("error: {}", e),
            }
        });
    }

    fn add_response_msg(&self, response_msg: Response) {
        let writer_queue = Arc::clone(&self.writer_assignments);
        if let Some(mut writer_queue_ref) = writer_queue.get_mut(&response_msg.client_id) {
            writer_queue_ref.value_mut().push_back(response_msg);
        } else {
            log::warn!("Client ID not found in writer assignments");
        };
    }

    fn handle_hello(&self, cmd: Hello, client_id: Uuid) {
        let mut tcp_client = self
            .connections
            .get_mut(&client_id)
            .expect("Client should exist");
        tcp_client.authenticate(cmd.user, cmd.protocol_version);
    }

    fn handle_lpop(&self, cmd: LPop, uuid: Uuid, queue_map: &DashMap<String, Lifo>) {
        let queue_name = cmd.key;
        let pop_size = cmd.count.unwrap_or(1) as usize;
        let mut queue = queue_map
            .entry(queue_name.clone())
            .or_insert_with(|| Lifo::new(queue_name));

        let messages = queue.pop(pop_size);
        for msg in messages {
            let response = Response::from_msg(uuid, msg);
            self.add_response_msg(response);
        }
    }

    fn handle_lpush(&self, cmd: LPush, uuid: Uuid, queue_map: &DashMap<String, Lifo>) {
        let mut queue = queue_map
            .entry(cmd.key.clone())
            .or_insert_with(|| Lifo::new(cmd.key.clone()));

        for element in cmd.elements {
            let msg = Message::new(element);
            queue.value_mut().add(msg);
        }

        let response = Response::from_usize(uuid, queue.value().size());
        self.add_response_msg(response);
    }

    pub async fn handle_new_stream(self: Arc<Self>, stream: TcpStream) {
        let client_id = Uuid::new_v4();
        let peer_addr = stream.peer_addr().unwrap();
        let (reader, writer) = stream.into_split();
        let client = TcpClient::new(peer_addr.to_string(), client_id);
        let cancellation_token = CancellationToken::new();

        self.cancellation_tokens
            .insert(client_id, cancellation_token);
        self.connections.insert(client_id, client);
        self.readers.insert(client_id, reader);
        self.writers.insert(client_id, writer);

        let read_thread = Arc::clone(&self);
        let write_thread = Arc::clone(&self);
        let read_handle = tokio::spawn(read_thread.process_client_read(client_id));
        let write_handle = tokio::spawn(write_thread.process_client_write(client_id));

        self.write_handles
            .insert(client_id, write_handle.abort_handle());
        self.read_handles
            .insert(client_id, read_handle.abort_handle());

        let (read_result, write_result) = tokio::join!(read_handle, write_handle);
        if let Err(e) = read_result {
            log::error!("Read task failed for client {}: {}", client_id, e);
        }
        if let Err(e) = write_result {
            log::error!("Write task failed for client {}: {}", client_id, e);
        }
        self.connections.remove(&client_id);
    }

    pub async fn process_client_write(self: Arc<Self>, client_id: Uuid) {
        let cancellation_token = self.cancellation_tokens.get(&client_id).unwrap();
        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    break;
                }
                continue_loop = async {
                    match self.connections.get(&client_id) {
                        None => return false,
                        Some(_) => {}
                    }
                    let q = match self.writer_assignments.get(&client_id) {
                        Some(q) => q,
                        None => return false,
                    };
                    if q.len() == 0 {
                        return true;
                    }
                    let mut q = match self.writer_assignments.get_mut(&client_id) {
                        Some(q) => q,
                        None => return false,
                    };
                    let last_q = q.pop_front().unwrap();
                    // drop early so we don't hold a lock while we write
                    drop(q);
                    return match self.writers.get_mut(&client_id) {
                        None => false,
                        Some(mut writer) => match writer.write_all(&last_q.data).await {
                            Ok(_) => true,
                            Err(err) => {
                                println!("error writing to writer: {}", err);
                                false
                            }
                        },
                    }
                } => {
                    if !continue_loop {
                        break;
                    }
                }
            }
            // sleep to let other threads have time to do work.
            sleep(std::time::Duration::from_millis(10)).await;
        }

        self.writers.remove(&client_id);
        // If there's anything left in the write_assignments, write it back into the lifo queue so we
        // don't lose it.
        match self.writer_assignments.get_mut(&client_id) {
            None => (),
            Some(mut q) => {
                while q.len() > 0 {
                    let msg = q.pop_front().unwrap();
                    if let Some(id) = msg.data_id {
                        let mut lifo_queue = self.queue_map.get_mut(&id).unwrap();
                        lifo_queue.reset_msg(&id);
                    }
                }
            }
        }
    }

    async fn process_client_read(self: Arc<Self>, client_id: Uuid) {
        let trx = &self.cmd_trx.clone();
        let mut msg_queue = VecDeque::new();
        loop {
            let client_ref = match self.readers.get(&client_id) {
                Some(client) => client,
                None => break, // Client no longer exists
            };
            match client_ref.readable().await {
                Ok(_) => {
                    let mut data: [u8; 4096] = [0; 4096];
                    match client_ref.try_read(&mut data) {
                        Ok(0) => break,
                        Ok(read) => {
                            let mut tcpClient = self.connections.get_mut(&client_id).unwrap();
                            tcpClient.read_buff(&mut data, read - 1).unwrap();
                            if (tcpClient.raw_msg_queue.len() > 0) {
                                let resp_msg = tcpClient.raw_msg_queue.pop_front().unwrap();
                                let _ = &msg_queue.push_back(resp_msg);
                            }
                        }
                        Err(e) => {
                            // TODO: handle
                            println!("error: {}", e);
                        }
                    }
                }
                Err(e) => {
                    // TODO: handle
                    println!("error reading stream: {}", e);
                    break;
                }
            }

            while &msg_queue.len() > &0 {
                let resp_msg = msg_queue.pop_front().unwrap();

                let cmd = Cmd::new(resp_msg).unwrap();
                trx.send((cmd, client_id)).unwrap();
            }
        }
        self.readers.remove(&client_id);
    }

    fn shutdown(self: Arc<Self>) {
        let shutdown_cmd = Cmd::Shutdown;
        let _ = &self.cmd_trx.send((shutdown_cmd, Uuid::new_v4())).unwrap();

        self.write_handles.iter().for_each(|h| {
            h.abort();
        });
    }
}
