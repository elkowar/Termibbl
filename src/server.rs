
//https://github.com/snapview/tokio-tungstenite/blob/master/examples/server.rs

use crate::data::Message;
use futures_util::stream::stream::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::stream::StreamExt;

#[derive(Debug, Serialize, Deserialize)]
pub enum ToClientMsg {
    NewMessage(Message),
}
#[derive(Debug, Serialize, Deserialize)]
pub enum ToServerMsg {
    Hello(String),
    NewMessage(String),
}

#[derive(Debug)]
struct ServerState {
    users: Vec<String>,
}

impl ServerState {
    fn new() -> Self {
        users = Vec::new()
    }
    fn apply_event(&mut self, msg: ToServerMsg) {
        println!("{:?}", msg);
    }
}

pub async fn run_server() {
    let server = TcpListener::bind("localhost:8080").await.unwrap();
    let (to_server_send, to_server_recv) = tokio::sync::mpsc::unbounded_channel::<ToServerMsg>();
    tokio::spawn(async move {
        let state = ServerState::new();
        select! {
            msg = to_server_recv.recv() => {
                state.apply_event(msg)
            }
        }
    });
    for stream in server.accept().await {
        tokio::spawn({ handle_connection(stream.0, to_server_send) });
    }
}

async fn handle_connection(
    raw_stream: TcpStream,
    to_server_send: tokio::sync::mpsc::Sender<ToServerMsg>,
) {
    let mut ws_stream = tokio_tungstenite::accept_async(raw_stream).await.unwrap();
    let (wsd_send, ws_read) = ws_stream.split();
    loop {
        ws_read()
        if msg.is_text() {
            to_server_send.send(serde_json::from_str(&msg.into_text().unwrap()).unwrap());
        }
    }
}
