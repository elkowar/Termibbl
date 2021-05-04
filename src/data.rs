use std::{
    cmp::max,
    collections::HashMap,
    hash::{Hash, Hasher},
};

use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display};
use tui::style::Color as TuiColor;

use crate::utils;

pub type UserId = u8;

#[derive(Default, Eq, Clone, Serialize, Deserialize, Ord, PartialOrd)]
pub struct Username(String, UserId);

impl Username {
    pub fn new(name: String, id: UserId) -> Username { Username(name, id) }
    pub fn name(&self) -> &str { self.0.as_str() }
    pub fn id(&self) -> UserId { self.1 }
    pub fn into_inner(self) -> (String, UserId) { (self.0, self.1) }
}

impl From<(&str, UserId)> for Username {
    fn from((name, id): (&str, UserId)) -> Self { Self::new(name.to_owned(), id) }
}

impl From<Username> for (String, UserId) {
    fn from(val: Username) -> Self { (val.0, val.1) }
}

impl Hash for Username {
    fn hash<H: Hasher>(&self, state: &mut H) { self.id().hash(state) }
}

impl PartialEq for Username {
    fn eq(&self, other: &Self) -> bool { self.id() == other.id() }
}

impl Display for Username {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl Debug for Username {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}#{}", self.name(), self.id())
    }
}

/// A u16 point in 2D space.
pub type Coord = (u16, u16);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameOpts {
    pub dimensions: Coord,
    pub number_of_rounds: usize,
    pub draw_time: usize,
    pub custom_words: Vec<String>,
    pub only_custom_words: bool,
    // pub canvas_bg_color: Color,
}

/// The data server stores for every player in a game
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerData {
    pub name: Username,
    pub score: usize,
    pub secs_to_solve_turn: u64,
}

impl From<Username> for PlayerData {
    fn from(name: Username) -> Self {
        Self {
            name,
            score: 0,
            secs_to_solve_turn: 0,
        }
    }
}

impl PlayerData {
    pub fn solved_current_round(&self) -> bool { self.secs_to_solve_turn != 0 }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WordHint {
    Draw(String),
    Hint {
        hints: HashMap<usize, char>, // revealed characters
        word_len: usize,
    },
}

impl From<&str> for WordHint {
    fn from(word: &str) -> Self {
        Self::Hint {
            word_len: word.len(),
            hints: word
                .chars()
                .enumerate()
                .filter(|(_, c)| c.is_whitespace() || c == &'-') // reveal whitespace and '-' chars
                .collect(),
        }
    }
}

impl WordHint {
    pub fn to_draw(&self) -> Option<&String> {
        match self {
            WordHint::Draw(word) => Some(&word),
            WordHint::Hint { .. } => None,
        }
    }
}

/// list of states a turn could be in
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TurnPhase {
    ChoosingWord(Vec<String>), // words to choose from, TODO: hide this from players
    Drawing(WordHint),
    RevealWord {
        word: String,
        scores: Vec<(Username, usize)>,
        timed_out: bool,
    },
}

impl TurnPhase {
    pub fn as_drawing(&self) -> Option<&WordHint> {
        if let Self::Drawing(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Turn {
    pub phase: TurnPhase,
    pub who_is_drawing: UserId,
}

/// list of states a game could be in
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GameState {
    RoundStart(usize),
    Playing(Turn),
    Finish,
}

impl GameState {
    pub fn as_turn(&self) -> Option<&Turn> {
        if let Self::Playing(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_turn_drawing_mut(&mut self) -> Option<&mut WordHint> {
        if let GameState::Playing(ref mut t) = self {
            if let TurnPhase::Drawing(hint) = &mut t.phase {
                return Some(hint);
            }
        }
        None
    }
}

/// Contains all info about an ongoing game
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameInfo {
    pub dimensions: (u16, u16),
    pub state: GameState,
    pub round_num: usize,
    pub next_phase_timestamp: u64,
    pub num_of_rounds: usize,
    pub players: Vec<PlayerData>,
    pub canvas: HashMap<Coord, Color>, // map of coord-color pairs sent to the server.
}

impl GameInfo {
    pub fn get_player(&self, id: UserId) -> Option<&PlayerData> {
        self.players.iter().find(|p| p.name.id() == id)
    }

    pub fn get_player_mut(&mut self, id: UserId) -> Option<&mut PlayerData> {
        self.players.iter_mut().find(|p| p.name.id() == id)
    }

    pub fn who_is_drawing(&self) -> Option<&Username> {
        self.state.as_turn().and_then(|t| {
            self.players
                .iter()
                .map(|pl| &pl.name)
                .find(|name| name.id() == t.who_is_drawing)
        })
    }

    pub fn did_state_timeout(&self) -> bool { self.next_phase_timestamp <= utils::get_time_now() }

    pub fn remaining_secs_in_phase(&self) -> u64 {
        max(
            0,
            self.next_phase_timestamp as i64 - utils::get_time_now() as i64,
        ) as u64
    }
}

macro_rules! derive_into {
    ($(#[$meta:meta])*
       $vis:vis enum $name: ident => $into_type: path { $($variant: ident => $into_variant: expr,)* }
     ) => {
        $(#[$meta])*
        $vis enum $name {
            $($variant),*
        }

        impl From<$name> for $into_type {
            fn from(v: $name) -> Self {
                match v {
                    $($name::$variant => $into_variant,)*
                }
            }
        }
    };
}

derive_into! {
    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum Color => TuiColor {
        White => TuiColor::White,
        Gray => TuiColor::Gray,
        DarkGray => TuiColor::DarkGray,
        Black => TuiColor::Black,
        Red => TuiColor::Red,
        LightRed => TuiColor::LightRed,
        Green => TuiColor::Green,
        LightGreen => TuiColor::LightGreen,
        Blue => TuiColor::Blue,
        LightBlue => TuiColor::LightBlue,
        Yellow => TuiColor::Yellow,
        LightYellow => TuiColor::LightYellow,
        Cyan => TuiColor::Cyan,
        LightCyan => TuiColor::LightCyan,
        Magenta => TuiColor::Magenta,
        LightMagenta => TuiColor::LightMagenta,
    }
}
