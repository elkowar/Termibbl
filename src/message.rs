use crate::{data, server::skribbl::SkribblState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ToClientMsg {
    NewMessage(data::Message),
    NewLine(data::Line),
    InitialState(InitialState),
    SkribblStateChanged(SkribblState),
    GameOver(SkribblState),
    ClearCanvas,
    TimeChanged(u32),
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ToServerMsg {
    NewMessage(data::Message),
    CommandMsg(data::CommandMsg),
    NewLine(data::Line),
    ClearCanvas,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InitialState {
    pub lines: Vec<data::Line>,
    pub dimensions: (usize, usize),
    pub skribbl_state: Option<SkribblState>,
}
