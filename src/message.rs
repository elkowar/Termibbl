use std::fmt::{self, Debug};

use serde::{Deserialize, Serialize};

use crate::data::{Color, Coord, GameInfo, GameOpts, GameState, PlayerData, Username};

/// number of seconds between each heartbeat sent by client
pub const HEARTBEAT_INTERVAL: u64 = 4;

/// Client -> Server
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ToServer {
    Heartbeat,
    Chat(ChatMessage),
    Draw(Draw),
    RequestRoom(Option<String>, RoomRequest), // optional nick & request
    LeaveRoom,
    Disconnect,
}

/// Server -> Client
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ToClient {
    RoomEvent(RoomEvent),
    JoinRoom(InitialRoomState),
    LeaveRoom(Option<String>), // reason for leaving
    Disconnect(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RoomRequest {
    Find,
    Create,
    Join(String), // room-key
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Draw {
    Clear,
    Erase(Coord),
    Paint { points: Vec<Coord>, color: Color },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoomEvent {
    Chat(ChatMessage),
    GameEvent(GameEvent),
    StartGame(GameInfo),
    EndGame, // return players to room lobby
    UserJoin(Username),
    UserLeave(Username),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameEvent {
    PlayerJoin(PlayerData),
    PlayGuessed(Username),
    PlayerListUpdate(Vec<PlayerData>),
    StateUpdate(GameState),
    WordHint((usize, char)), // index and char to reveal
    Draw(Draw),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InitialRoomState {
    pub username: Username,
    pub room: RoomInfo,
    pub game: Option<GameInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoomInfo {
    pub key: String,
    pub connected_users: Vec<Username>,
    pub game_opts: GameOpts,
    pub leader: Option<Username>,
    // pub max_room_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatMessage {
    System(String),
    User(Username, String),
}

impl ChatMessage {
    pub fn is_system(&self) -> bool { matches!(self, ChatMessage::System(..)) }

    pub fn username(&self) -> Option<&Username> {
        match self {
            ChatMessage::User(username, _) => Some(username),
            _ => None,
        }
    }

    pub fn inner(&self) -> &str {
        match self {
            ChatMessage::System(msg) => &msg,
            ChatMessage::User(_, msg) => &msg,
        }
    }

    pub fn into_inner(self) -> String {
        match self {
            ChatMessage::System(msg) | ChatMessage::User(_, msg) => msg,
        }
    }
}

impl fmt::Display for ChatMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatMessage::System(msg) => write!(f, "{}", msg),
            ChatMessage::User(user, msg) => write!(f, "{}: {}", user, msg),
        }
    }
}
