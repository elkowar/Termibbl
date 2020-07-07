use super::server::ROUND_DURATION;
use crate::client::Username;
use rand::{prelude::IteratorRandom, seq::SliceRandom};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{cmp::max, time};
use time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkribblState {
    current_word: String,
    revealed_characters: Vec<usize>,

    /// the currently drawing user
    pub drawing_user: Username,

    /// players which didn't draw yet in the current round.
    pub remaining_users: Vec<Username>,

    /// states of all the players
    pub player_states: HashMap<Username, PlayerState>,

    pub round_end_time: u64,

    pub remaining_words: Vec<String>,
}

impl SkribblState {
    pub fn current_word(&self) -> &str {
        &self.current_word
    }

    pub fn revealed_characters(&self) -> &[usize] {
        self.revealed_characters.as_ref()
    }

    pub fn set_current_word(&mut self, word: String) {
        self.current_word = word;
        self.revealed_characters = Vec::new();
    }

    /// reveals a random character, as long as that doesn't reveal half of the word
    pub fn reveal_random_char(&mut self) {
        if self.revealed_characters.len() < self.current_word.len() / 2 {
            let mut rng = rand::thread_rng();
            self.revealed_characters
                .push((0..self.current_word.len()).choose(&mut rng).unwrap());
        }
    }

    /// returns the placeholder chars for the current word, with the revealed characters revealed.
    pub fn hinted_current_word(&self) -> String {
        self.current_word
            .chars()
            .enumerate()
            .map(|(idx, c)| {
                if self.revealed_characters.contains(&(idx as usize)) {
                    c
                } else if c.is_whitespace() {
                    ' '
                } else {
                    '?'
                }
            })
            .collect()
    }

    pub fn remaining_time(&self) -> u32 {
        max(0, self.round_end_time as i64 - get_time_now() as i64) as u32
    }

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

    pub fn next_turn(&mut self) -> &Username {
        let remaining_time = self.remaining_time();
        self.player_states
            .get_mut(&self.drawing_user)
            .map(|drawing_user| {
                drawing_user.score += 50;
                drawing_user.on_solve(remaining_time);
            });

        let new_word = self.remaining_words.remove(0);
        self.set_current_word(new_word);
        self.round_end_time = get_time_now() + ROUND_DURATION;
        if self.remaining_users.len() == 0 {
            self.remaining_users = self.player_states.keys().cloned().collect();
        }
        self.drawing_user = self.remaining_users.remove(0);
        self.player_states
            .iter_mut()
            .for_each(|(_, player)| player.has_solved = false);
        &self.drawing_user
    }

    pub fn new(users: Vec<Username>, mut words: Vec<String>) -> Self {
        let mut rng = rand::thread_rng();
        words.shuffle(&mut rng);
        let current_word = words.remove(0);
        let mut state = SkribblState {
            current_word: current_word.clone(),
            revealed_characters: Vec::new(),
            drawing_user: users[0].clone(),
            remaining_users: users.iter().cloned().skip(1).collect::<Vec<_>>(),
            player_states: HashMap::new(),
            round_end_time: get_time_now() + ROUND_DURATION,
            remaining_words: words,
        };
        for user in users {
            state.player_states.insert(user, PlayerState::default());
        }
        state
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
    pub fn on_solve(&mut self, remaining_time: u32) {
        self.score += calculate_score_increase(remaining_time);
        self.has_solved = true;
    }
}

pub fn calculate_score_increase(remaining_time: u32) -> u32 {
    50 + (((remaining_time as f64 / ROUND_DURATION as f64) * 100f64) as u32 / 2u32)
}
