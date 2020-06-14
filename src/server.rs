use crate::data::Message;
use serde::{Deserialize, Serialize};
use std::{net::TcpListener, thread::spawn};

#[derive(Debug, Serialize, Deserialize)]
pub enum ToClientMsg {
    NewMessage(Message),
}
#[derive(Debug, Serialize, Deserialize)]
pub enum ToServerMsg {
    Hello(String),
    NewMessage(String),
}

pub async fn run_server() {
    let server = TcpListener::bind("localhost:8080").unwrap();
    for stream in server.incoming() {
        spawn(move || {
            let mut websocket = tungstenite::accept(stream.unwrap()).unwrap();
            let mut name = String::new();
            loop {
                let msg = websocket.read_message().unwrap();
                if msg.is_text() {
                    println!("{:?}", msg);
                    let msg: Result<ToServerMsg, _> =
                        serde_json::from_str(&msg.into_text().unwrap());
                    match msg {
                        Ok(ToServerMsg::Hello(n)) => name = n.to_string(),
                        Ok(ToServerMsg::NewMessage(msg)) => println!("{} - {}", name, msg),
                        _ => println!("Got unhandled message: {:?}", msg),
                    }
                }
            }
        });
    }
}
