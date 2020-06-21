use crate::data;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ToClientMsg {
    UserJoined(String),
    NewMessage(data::Message),
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ToServerMsg {
    NewMessage(data::Message),
}
