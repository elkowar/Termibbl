use std::{collections::HashMap, sync::Arc, time::Duration};

use GameEvent::StateUpdate;

use crate::{
    data::{GameState, TurnPhase, UserId, Username, WordHint},
    events::{EventQueue, EventSender},
    message::{ChatMessage, Draw, GameEvent, InitialRoomState, RoomEvent, RoomInfo},
    // utils,
};

use super::{
    session::{
        Message::{RoomClosed, RoomEvent as Event, RoomJoined},
        UserSessionInbox,
    },
    skribbl::SkribblState,
    Error, GameOpts, Result,
};

const REQUIRED_PLAYERS: usize = 2;

pub type RoomInbox = EventSender<RoomMessage>;

/// List of messages a game room can recieve
pub enum RoomMessage {
    /// Notify room of player join
    Join {
        name: Username,
        inbox: UserSessionInbox,
    },

    /// Notify room of player leaving
    Leave { name: Username },

    /// Notify room of draw message
    Draw { from: Username, draw: Draw },

    /// Notify room of player chat
    Chat { from: Username, msg: String },

    /// Close the room, stops the game room loop
    Close,

    /// Do a game tick
    Tick,
}

pub struct GameRoom {
    /// room key
    key: String,

    /// the leader of this room
    leader: Option<Username>,

    /// options of this room
    game_opts: GameOpts,

    /// The main server thread creates the shared Vec of words and passes a reference to the rooms.
    shared_server_words: Arc<Vec<String>>,

    /// holds all sessions connected to this room
    sessions: HashMap<Username, UserSessionInbox>,

    /// event queue for this room loop
    event_queue: EventQueue<RoomMessage>,

    /// game struct
    skribbl: Option<SkribblState>,
}

impl From<GameState> for RoomEvent {
    fn from(val: GameState) -> Self { RoomEvent::GameEvent(GameEvent::StateUpdate(val)) }
}

impl From<GameEvent> for RoomEvent {
    fn from(val: GameEvent) -> Self { RoomEvent::GameEvent(val) }
}

impl GameRoom {
    pub fn new(
        key: String,
        game_opts: GameOpts,
        server_words: &Arc<Vec<String>>,
        leader: Option<Username>,
    ) -> Self {
        Self {
            key,
            leader,
            game_opts,
            shared_server_words: Arc::clone(server_words),
            sessions: HashMap::new(),
            event_queue: EventQueue::default(),
            skribbl: None,
        }
    }

    pub fn key(&self) -> &str { &self.key }

    pub fn sender(&self) -> &EventSender<RoomMessage> { self.event_queue.sender() }

    /// send a `RoomEvent` to a specific session
    fn send<E: Into<RoomEvent>>(&self, name: Username, event: E) {
        let event = event.into();
        if let Some(session) = self.sessions.get(&name) {
            session.send(Event(event));
        }
    }

    /// send a `ChatMessage::System` to a specific session
    fn send_system_msg<T: Into<String>>(&self, user: Username, msg: T) {
        if let Some(session) = self.sessions.get(&user) {
            session.send(Event(RoomEvent::Chat(ChatMessage::System(msg.into()))));
        }
    }

    /// send a ChatMessage::SystemMsg to all active sessions in room
    fn broadcast_msg(&self, msg: ChatMessage) { self.broadcast(RoomEvent::Chat(msg)) }

    /// send a ChatMessage::SystemMsg to all active sessions in room
    pub fn broadcast_system_msg(&self, msg: String) {
        self.broadcast(RoomEvent::Chat(ChatMessage::System(msg)))
    }

    /// broadcast a `RoomEvent` to all connected players
    fn broadcast<E: Into<RoomEvent>>(&self, event: E) {
        let event = event.into();
        for (name, session) in self.sessions.iter() {
            session.send(Event(event.clone()));
        }
    }

    /// broadcast a `RoomEvent` to all connected players excluding given player
    fn broadcast_except<E: Into<RoomEvent>>(&mut self, event: E, except: UserId) {
        let event = event.into();
        for (_, session) in self.sessions.iter_mut().filter(|(n, _)| n.id() != except) {
            session.send(Event(event.clone()));
        }
    }

    fn users(&self) -> Vec<Username> { self.sessions.keys().cloned().collect() }

    fn info(&self) -> RoomInfo {
        RoomInfo {
            key: self.key.clone(),
            connected_users: self.users(),
            game_opts: self.game_opts.clone(),
            leader: self.leader.clone(),
        }
    }

    fn start_game(&mut self) {
        if self.skribbl.is_some() {
            return log::warn!("room tried to start game when already started",);
        }

        if self.game_opts.only_custom_words && self.game_opts.custom_words.is_empty() {
            // cannot start game with no guess words
            return;
        }

        self.skribbl = Some({
            // create game with current game_opts
            let game = SkribblState::new(
                self.game_opts.clone(),
                self.users(),
                Arc::clone(&self.shared_server_words),
            );

            // broadcast this game to all players
            self.broadcast(RoomEvent::StartGame(game.info.clone()));

            game
        });

        // start game ticks
        self.event_queue
            .sender()
            .send_with_delay(RoomMessage::Tick, Duration::from_secs(1));
    }

    fn end_game(&mut self) {
        if self.skribbl.take().is_some() {
            if self
                .leader
                .as_ref()
                .map(|leader| self.sessions.contains_key(leader))
                .unwrap_or_default()
            {
                self.broadcast(RoomEvent::EndGame);
            } else {
                // close this room if does not have a leader that is still connected
                self.sender().send_with_urgency(RoomMessage::Close);
            }

            log::debug!("Ending game room {}.", self.key);
        } else {
            log::warn!("tried to end game in room {} with no game.", self.key);
        };
    }

    fn on_paint_msg(&mut self, sender: Username, draw: Draw) {
        if let Some(ref mut game) = self.skribbl {
            // only process draw message from player that can draw
            if !game.is_drawing(sender.id()) {
                return; // naughty client
            }

            // update server game state
            let canvas = &mut game.info.canvas;
            match &draw {
                Draw::Clear => canvas.clear(),
                Draw::Paint { points, color } => {
                    for point in points {
                        canvas.insert(*point, *color);
                    }
                }
                Draw::Erase(point) => {
                    canvas.remove(point);
                }
            };

            self.broadcast_except(RoomEvent::GameEvent(GameEvent::Draw(draw)), sender.id());
        }
    }

    fn on_chat_msg(&mut self, sender: Username, chat_msg: String) {
        if let Some(ref mut game) = self.skribbl {
            // whether the given player can guess in the current turn.
            if game.can_player_guess(game.get_player(&sender).unwrap()) {
                match game.do_guess(&sender, &chat_msg) {
                    // TODO: on correct guess, let users know that score has gone up?
                    0 => self.broadcast_system_msg(format!("{} guessed it!", sender)),

                    1 => self.send_system_msg(sender, "You're very close!".to_string()),
                    _ => self.broadcast_msg(ChatMessage::User(sender, chat_msg)),
                };
            } else {
                // player cannot guess, send message to all users who can't
                let msg = RoomEvent::Chat(ChatMessage::User(sender, chat_msg));

                for player in game
                    .get_non_guessing_players()
                    .iter()
                    .map(|pl| pl.name.clone())
                    .collect::<Vec<Username>>()
                {
                    self.send(player, msg.clone());
                }
            }
        } else {
            // let everyone know message has been sent
            self.broadcast_msg(ChatMessage::User(sender, chat_msg));
        }
    }

    fn on_user_leave(&mut self, username: Username) {
        if self.sessions.remove(&username).is_some() {
            let id = username.id();

            log::info!("({}) {} has left the room.", self.key, username);

            // maybe let the client handle the message?
            self.broadcast_system_msg(format!("{} left the room", username));
            self.broadcast(RoomEvent::UserLeave(username));

            if let Some(ref mut game) = self.skribbl {
                if self.sessions.is_empty() {
                    self.end_game();
                } else if self.sessions.len() < REQUIRED_PLAYERS {
                    // stop game when there isnt enough players
                    game.end();
                    let state = game.info.state.clone();
                    self.broadcast(StateUpdate(state));
                } else if game.is_drawing(id) {
                    // skip turn if this user is drawing
                    self.start_next_turn();
                }
            }
        }
    }

    fn on_user_join(&mut self, username: Username, inbox: UserSessionInbox) {
        if self.sessions.contains_key(&username) {
            return log::warn!(
                "{} tried to join room `{}` they are already in",
                username,
                self.key
            );
        }

        // send joining player initial game state
        inbox.send_with_urgency(RoomJoined(
            self.sender().clone(),
            InitialRoomState {
                username: username.clone(),
                room: self.info(),
                game: self.skribbl.as_ref().map(|game| game.info.clone()),
            },
        ));

        let join_msg = format!("{} joined", username);

        // update all users on this user
        self.broadcast(RoomEvent::UserJoin(username.clone()));
        self.sessions.insert(username, inbox);

        // is this neccesary?, could be done on client.
        self.broadcast_system_msg(join_msg);

        // start game if there are enough players and no room leader
        if self.leader.is_none() && self.sessions.len() >= REQUIRED_PLAYERS {
            self.start_game()
        }
    }

    fn reveal_word(&mut self, timed_out: bool) {
        let game = if let Some(ref mut game) = self.skribbl {
            game
        } else {
            return;
        };

        // reveal word
        game.end_turn(timed_out);

        let game_state = game.info.state.clone();

        // broadcast state
        self.broadcast(game_state);
    }

    fn choose_word(&mut self, choice: Option<String>) {
        let skribbl = match &mut self.skribbl {
            Some(it) => it,
            _ => return,
        };

        let maybe_turn_phase = skribbl.info.state.as_turn().map(|t| &t.phase);
        if let Some(TurnPhase::ChoosingWord(choices)) = maybe_turn_phase {
            let choice = choice.filter(|c| choices.contains(c));

            skribbl.choose_draw_word(choice);

            let word_to_draw = skribbl.word().to_owned();
            let mut turn = skribbl.info.state.as_turn().cloned().unwrap();
            let who_is_drawing = turn.who_is_drawing;

            // clear canvas
            skribbl.info.canvas.clear();
            self.broadcast(GameEvent::Draw(Draw::Clear));
            self.broadcast_except(
                StateUpdate(GameState::Playing(turn.clone())),
                who_is_drawing,
            );

            // send word to player that is drawing
            self.send(
                ("", who_is_drawing).into(), // id -> username
                GameState::Playing({
                    turn.phase = TurnPhase::Drawing(WordHint::Draw(word_to_draw));
                    turn
                }),
            );
        }
    }

    fn start_next_turn(&mut self) {
        if let Some(ref mut game) = self.skribbl {
            if game.has_round_ended() {
                log::debug!(
                    "(#{}) Round {} end.. starting next round",
                    self.key,
                    game.info.round_num
                );
                game.start_round();
                let game_state = game.info.state.clone();
                self.broadcast(StateUpdate(game_state));
            } else {
                game.start_next_turn();
                let game_state = game.info.state.clone();

                if let Some(who_is_drawing) = game.info.who_is_drawing() {
                    log::debug!("(#{}) Starting turn for {:?} ..", self.key, who_is_drawing);
                } else {
                    log::debug!("(#{}) Finshing game (showing winners) ..", self.key,);
                }

                self.broadcast(StateUpdate(game_state));
            }
        }
    }

    fn on_tick(&mut self) {
        let game = if let Some(ref mut game) = self.skribbl {
            game
        } else {
            return;
        };

        if game.info.did_state_timeout() {
            match &game.info.state {
                GameState::RoundStart(..) => self.start_next_turn(),
                GameState::Playing(turn) => match turn.phase {
                    TurnPhase::ChoosingWord(_) => self.choose_word(None),
                    TurnPhase::Drawing(_) => self.reveal_word(true),
                    TurnPhase::RevealWord { .. } => {
                        let players = game.info.players.clone();
                        self.broadcast(GameEvent::PlayerListUpdate(players));
                        self.start_next_turn();
                    }
                },
                GameState::Finish => self.end_game(),
            };
        } else if let GameState::Playing(turn) = &game.info.state {
            let who_is_drawing = turn.who_is_drawing;
            let players_left_to_guess = game
                .info
                .players
                .iter()
                .filter(|pl| who_is_drawing != pl.name.id() && pl.secs_to_solve_turn == 0)
                .count();

            if players_left_to_guess == 0 {
                game.end_turn(false);
            } else if let Some(revealed) = game.reveal_random_char() {
                self.broadcast_except(GameEvent::WordHint(revealed), who_is_drawing);
            }
        }

        // send another tick after a second
        self.event_queue
            .sender()
            .send_with_delay(RoomMessage::Tick, Duration::from_secs(1));
    }

    /// blocking loop
    pub async fn run_loop(&mut self) -> Result<()> {
        loop {
            match self
                .event_queue
                .recv_async()
                .await
                .ok_or(Error::EmptyOptional)?
            {
                RoomMessage::Tick => self.on_tick(),
                RoomMessage::Join { name, inbox } => self.on_user_join(name, inbox),
                RoomMessage::Leave { name } => self.on_user_leave(name),
                RoomMessage::Draw { from, draw } => self.on_paint_msg(from, draw),
                RoomMessage::Chat { from, msg } => self.on_chat_msg(from, msg),
                RoomMessage::Close => break,
            }
        }

        for (_, session) in self.sessions.drain() {
            session.send_with_urgency(RoomClosed)
        }

        Ok(())
    }
}
