use crate::data;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ToClientMsg {
    UserJoined(String),
    NewMessage(data::Message),
    NewLine(data::Line),
    InitialState(InitialState),
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ToServerMsg {
    NewMessage(data::Message),
    NewLine(data::Line),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InitialState {
    pub lines: Vec<data::Line>,
    pub current_users: Vec<String>,
    pub dimensions: (usize, usize),
}
