use crate::data;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ToClientMsg {
    UserJoined(String),
    NewMessage(data::Message),
    NewLine(data::Line),
    InitialState {
        lines: Vec<data::Line>,
        current_users: Vec<String>,
    },
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ToServerMsg {
    NewMessage(data::Message),
    NewLine(data::Line),
}
