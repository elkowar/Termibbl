use std::{cmp::min, sync::Arc};

use rand::{
    prelude::{IteratorRandom, SliceRandom, StdRng},
    SeedableRng,
};

use crate::{
    data::{
        GameInfo, GameOpts, GameState, PlayerData, Turn, TurnPhase, UserId, Username, WordHint,
    },
    utils,
};

pub const NUM_OF_WORDS_PER_TURN: usize = 3;
pub const DEFAULT_DRAW_TIME: u64 = 120;
pub const DEFAULT_NUM_OF_ROUNDS: usize = 3;
pub const CHOOSE_WORDS_TIME: u64 = 10; // num of seconds for players to choose word before timeout
pub const REVEAL_PHASE_SECS: u64 = 3; // num of seconds in reveal word phase

/// This struct is used to hold and collect words that can be used for
/// guessing in this game
struct WordProducer {
    /// random generator for this producer
    rng: StdRng,

    /// Vec of words that could be used in this producer
    words: Vec<String>,

    /// vec of additional words
    shared_words: Arc<Vec<String>>,

    num_of_words: usize,

    use_shared_words: bool,
}

impl WordProducer {
    // const WORDS_TO_SHARED_WEIGHT: (usize,usize) = (2, 1); // 66%, 33%

    fn new(
        words: Vec<String>,
        shared_words: Arc<Vec<String>>,
        use_shared_words: bool,
        num_of_words: usize,
    ) -> Self {
        Self {
            shared_words,
            words,
            use_shared_words,
            rng: SeedableRng::from_entropy(),
            num_of_words,
        }
    }

    fn rng(&mut self) -> &mut StdRng { &mut self.rng }

    fn next_custom(&mut self) -> Option<String> { None }
}

impl Iterator for WordProducer {
    type Item = Vec<String>;

    // TODO: use weights to choose words. just return shared words for now
    fn next(&mut self) -> Option<Self::Item> {
        Some(
            self.shared_words
                .choose_multiple(&mut self.rng, self.num_of_words)
                .cloned()
                .collect(),
        )
    }
}

/// Wrapper struct to handle server game events
pub struct SkribblState {
    /// the current game state
    pub info: GameInfo,

    /// producer of words to guess
    words: WordProducer,

    /// players whom havn't draw yet in the current round.
    players_left_in_round: Vec<UserId>,

    /// current word to guess
    current_word: String,

    /// number of seconds players have to draw
    draw_time: usize,
}

impl SkribblState {
    pub fn new(
        opts: GameOpts,
        users: Vec<Username>,
        shared_server_words: Arc<Vec<String>>,
    ) -> Self {
        let words = WordProducer::new(
            opts.custom_words,
            shared_server_words,
            !opts.only_custom_words,
            NUM_OF_WORDS_PER_TURN,
        );

        let info = GameInfo {
            dimensions: opts.dimensions,
            state: GameState::RoundStart(0),
            round_num: 0,
            next_phase_timestamp: 0,
            num_of_rounds: opts.number_of_rounds,
            players: users.into_iter().map(|username| username.into()).collect(),
            canvas: Default::default(),
        };

        let mut new = SkribblState {
            info,
            players_left_in_round: Vec::new(),
            draw_time: opts.draw_time,
            words,
            current_word: String::new(),
        };

        new.start_round();

        new
    }

    pub fn word(&self) -> &String { &self.current_word }

    pub fn has_round_ended(&self) -> bool { self.players_left_in_round.is_empty() }

    pub fn end(&mut self) {
        self.info.state = GameState::Finish;
        self.info.next_phase_timestamp = utils::get_time_now() + 5;
    }

    pub fn start_round(&mut self) {
        let round_num = &mut self.info.round_num;
        if *round_num >= self.info.num_of_rounds {
            // game is natually finished.
            self.end();
        } else {
            *round_num += 1;
            self.players_left_in_round = self.info.players.iter().map(|pl| pl.name.id()).collect();
            self.info.state = GameState::RoundStart(*round_num);
            self.info.next_phase_timestamp = utils::get_time_now() + 5;
        }
    }

    pub fn start_next_turn(&mut self) {
        if self.players_left_in_round.is_empty() {
            self.start_round();
            return;
        }

        // set next turn
        self.info.state = GameState::Playing(Turn {
            who_is_drawing: self.players_left_in_round.pop().unwrap(),
            phase: TurnPhase::ChoosingWord(self.words.next().unwrap()),
        });
        self.info.next_phase_timestamp = utils::get_time_now() + CHOOSE_WORDS_TIME;
        self.info.next_phase_timestamp = utils::get_time_now(); // skip choosing word for now
    }

    pub fn choose_draw_word(&mut self, word: Option<String>) {
        if let GameState::Playing(turn) = &mut self.info.state {
            let rng = self.words.rng();
            let word = if let TurnPhase::ChoosingWord(ref choices) = turn.phase {
                word.or_else(|| choices.choose(rng).cloned()).unwrap()
            } else {
                return;
            };

            // set next phase
            turn.phase = TurnPhase::Drawing(word.as_str().into());

            self.info.next_phase_timestamp = utils::get_time_now() + (self.draw_time as u64);
            self.current_word = word;
        }
    }

    pub fn end_turn(&mut self, timed_out: bool) {
        let draw_time = self.draw_time;
        let game_info = &mut self.info;
        // let num_of_players = game_info.players.len();
        let remaining_secs = game_info.remaining_secs_in_phase();

        if let GameState::Playing(turn) = &mut game_info.state {
            let who_is_drawing = turn.who_is_drawing;
            // sort by who solved quicker
            game_info
                .players
                .sort_by(|a, b| a.secs_to_solve_turn.cmp(&b.secs_to_solve_turn));

            // scoring algo needs more work
            let scores: Vec<(Username, usize)> = game_info
                .players
                .iter_mut()
                .enumerate()
                .map(|(idx, player)| {
                    let score = if player.name.id() == who_is_drawing {
                        (remaining_secs * (200 / draw_time) as u64) as usize + (5 * idx)
                    } else {
                        300 - (idx * player.secs_to_solve_turn as usize)
                    };

                    player.secs_to_solve_turn = 0;
                    player.score += score;
                    (player.name.clone(), score)
                })
                .collect();

            // set next phase
            turn.phase = TurnPhase::RevealWord {
                word: self.current_word.clone(),
                scores,
                timed_out,
            };
            self.info.next_phase_timestamp = utils::get_time_now() + REVEAL_PHASE_SECS;
        }
    }

    /// try guess for a player by username, returns distance of guess
    pub fn do_guess(&mut self, player_name: &Username, guess: &str) -> usize {
        let remaining_secs = self.info.remaining_secs_in_phase();
        let draw_time = self.draw_time;
        let dist = levenshtein_distance(guess, &self.current_word);

        if let Some(player) = self.get_player_mut(player_name) {
            if dist == 0 {
                player.secs_to_solve_turn = draw_time as u64 - remaining_secs;
            }

            dist
        } else {
            log::warn!("player `{}` tried to guess but not in game", player_name);
            2
        }
    }

    pub fn get_player(&self, name: &Username) -> Option<&PlayerData> {
        self.info.players.iter().find(|pl| &pl.name == name)
    }

    pub fn get_player_mut(&mut self, name: &Username) -> Option<&mut PlayerData> {
        self.info.players.iter_mut().find(|pl| &pl.name == name)
    }

    pub fn get_non_guessing_players(&self) -> Vec<&PlayerData> {
        self.info
            .players
            .iter()
            .filter(|pl| !self.can_player_guess(pl))
            .collect()
    }

    pub fn can_player_guess(&self, pl: &PlayerData) -> bool {
        !(pl.secs_to_solve_turn != 0 || self.is_drawing(pl.name.id()))
    }

    pub fn is_drawing(&self, id: UserId) -> bool {
        self.info
            .state
            .as_turn()
            .map(|turn| turn.who_is_drawing == id)
            .unwrap_or(false)
    }

    /// reveals a random character, as long as that doesn't reveal all of the word
    /// returns the index and character hinted if any
    pub fn reveal_random_char(&mut self) -> Option<(usize, char)> {
        let remaining_time = self.info.remaining_secs_in_phase();
        let num_of_chars_to_reveal = self.current_word.len() - 1;

        if let Some(WordHint::Hint { hints, .. }) = &mut self.info.state.as_turn_drawing_mut() {
            let should_reveal_char = {
                if num_of_chars_to_reveal <= 1 {
                    false
                } else {
                    let char_reveal_interval = self.draw_time / num_of_chars_to_reveal;

                    remaining_time as usize / char_reveal_interval
                        <= num_of_chars_to_reveal - hints.len()
                }
            };

            if should_reveal_char {
                let (idx, ch) = self
                    .current_word
                    .chars()
                    .enumerate()
                    .filter(|(idx, _)| !hints.contains_key(&idx))
                    .choose(self.words.rng())
                    .unwrap();

                hints.insert(idx, ch);

                return Some((idx, ch));
            }
        }

        None
    }
}

fn levenshtein_distance(a: &str, b: &str) -> usize {
    let w1 = a.chars().collect::<Vec<_>>();
    let w2 = b.chars().collect::<Vec<_>>();

    let a_len = w1.len() + 1;
    let b_len = w2.len() + 1;

    let mut matrix = vec![vec![0]];

    for i in 1..a_len {
        matrix[0].push(i);
    }
    for j in 1..b_len {
        matrix.push(vec![j]);
    }

    for (j, i) in (1..b_len).flat_map(|j| (1..a_len).map(move |i| (j, i))) {
        let x: usize = if w1[i - 1].eq_ignore_ascii_case(&w2[j - 1]) {
            matrix[j - 1][i - 1]
        } else {
            1 + min(
                min(matrix[j][i - 1], matrix[j - 1][i]),
                matrix[j - 1][i - 1],
            )
        };
        matrix[j].push(x);
    }
    matrix[b_len - 1][a_len - 1]
}
