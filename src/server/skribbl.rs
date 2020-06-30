use crate::client::Username;
use rand::seq::IteratorRandom;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time;
use time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkribblState {
    pub current_word: String,

    /// the currently drawing user
    pub drawing_user: Username,

    /// players which didn't draw yet in the current round.
    pub remaining_users: Vec<Username>,

    /// states of all the players
    pub player_states: HashMap<Username, PlayerState>,

    pub round_start_time: u64,
}

impl SkribblState {
    pub fn did_all_solve(&self) -> bool {
        self.player_states
            .iter()
            .all(|(username, player)| player.has_solved || username == &self.drawing_user)
    }

    pub fn has_solved(&self, username: &Username) -> bool {
        self.player_states.get(username).map(|x| x.has_solved) == Some(true)
    }

    pub fn remove_user(&mut self, username: &Username) {
        self.player_states.remove(username);
        let left_player_idx = self
            .remaining_users
            .iter()
            .enumerate()
            .find(|(_, name)| name == &username)
            .map(|x| x.0);
        // TODO check if idx = 0
        if let Some(idx) = left_player_idx {
            self.remaining_users.remove(idx);
        }
    }

    pub fn add_player(&mut self, username: Username) {
        if !self.player_states.contains_key(&username) {
            self.remaining_users.push(username.clone());
            self.player_states.insert(username, PlayerState::default());
        }
    }

    pub fn next_player(&mut self, word: String) -> &Username {
        self.round_start_time = get_time_now();
        if self.remaining_users.len() == 0 {
            self.remaining_users = self.player_states.keys().cloned().collect();
        }
        self.drawing_user = self.remaining_users.remove(0);
        self.player_states
            .iter_mut()
            .for_each(|(_, player)| player.has_solved = false);
        self.current_word = word;
        &self.drawing_user
    }

    pub fn with_users(users: Vec<Username>, words: &[String]) -> Self {
        let mut rng = rand::thread_rng();
        let mut state = SkribblState {
            current_word: words.iter().choose(&mut rng).unwrap().clone(),
            drawing_user: users[0].clone(),
            remaining_users: users.iter().cloned().skip(1).collect::<Vec<_>>(),
            player_states: HashMap::new(),
            round_start_time: get_time_now(),
        };
        for user in users {
            state.player_states.insert(user, PlayerState::default());
        }
        state
    }

    pub fn is_drawing(&self, username: &Username) -> bool {
        self.drawing_user == *username
    }
    pub fn can_guess(&self, username: &Username) -> bool {
        !self.is_drawing(username)
            && !self
                .player_states
                .get(username)
                .map(|x| x.has_solved)
                .unwrap_or(false)
    }
}

pub fn get_time_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlayerState {
    pub score: u32,
    pub has_solved: bool,
}

impl Default for PlayerState {
    fn default() -> Self {
        PlayerState {
            score: 0,
            has_solved: false,
        }
    }
}

impl PlayerState {
    pub fn on_solve(&mut self) {
        self.score += 100;
        self.has_solved = true;
    }
}
