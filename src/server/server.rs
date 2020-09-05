//https://github.com/snapview/tokio-tungstenite/blob/master/examples/server.rs

use super::skribbl::SkribblState;
use crate::{
    data,
    message::{InitialState, ToClientMsg, ToServerMsg},
};
use data::{CommandMsg, Message, Username};
use futures_timer::Delay;
use futures_util::{SinkExt, StreamExt};
use std::io::Read;
use std::net::SocketAddr;
use std::{cmp::min, collections::HashMap, path::PathBuf, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
};

pub const ROUND_DURATION: u64 = 120;

type Result<T> = std::result::Result<T, ServerError>;

#[derive(Debug)]
pub enum ServerError {
    UserNotFound(String),
    SendError(String),
    WsError(tungstenite::error::Error),
    IOError(std::io::Error),
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for ServerError {
    fn from(err: tokio::sync::mpsc::error::SendError<T>) -> Self {
        ServerError::SendError(err.to_string())
    }
}

impl From<tungstenite::error::Error> for ServerError {
    fn from(err: tungstenite::error::Error) -> Self {
        ServerError::WsError(err)
    }
}

impl From<std::io::Error> for ServerError {
    fn from(err: std::io::Error) -> Self {
        ServerError::IOError(err)
    }
}

#[derive(Debug)]
enum ServerEvent {
    ToServerMsg(Username, ToServerMsg),
    UserJoined(UserSession),
    UserLeft(Username),
    Tick,
}

#[derive(Debug)]
struct UserSession {
    username: Username,
    msg_send: Mutex<tokio::sync::mpsc::Sender<ToClientMsg>>,
    close_send: tokio::sync::mpsc::Sender<()>,
}

impl UserSession {
    fn new(
        username: Username,
        msg_send: tokio::sync::mpsc::Sender<ToClientMsg>,
        close_send: tokio::sync::mpsc::Sender<()>,
    ) -> Self {
        UserSession {
            username,
            msg_send: Mutex::new(msg_send),
            close_send,
        }
    }

    async fn close(mut self) -> Result<()> {
        self.close_send.send(()).await?;
        Ok(())
    }

    async fn send(&self, msg: ToClientMsg) -> Result<()> {
        self.msg_send.lock().await.send(msg.clone()).await?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum GameState {
    FreeDraw,
    Skribbl(SkribblState),
}

impl GameState {
    fn skribbl_state(&self) -> Option<&SkribblState> {
        match self {
            GameState::Skribbl(state) => Some(state),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct ServerState {
    sessions: HashMap<Username, UserSession>,
    pub lines: Vec<data::Line>,
    pub dimensions: (usize, usize),
    pub game_state: GameState,
    pub words: Option<Vec<String>>,
}

impl ServerState {
    fn new(game_state: GameState, dimensions: (usize, usize), words: Option<Vec<String>>) -> Self {
        ServerState {
            sessions: HashMap::new(),
            lines: Vec::new(),
            dimensions,
            game_state,
            words,
        }
    }

    async fn remove_player(&mut self, username: &Username) -> Result<()> {
        self.sessions.remove(username).map(|x| x.close());
        let state = match &mut self.game_state {
            GameState::Skribbl(state) => state,
            _ => return Ok(()),
        };
        if state.is_drawing(username) {
            state.next_turn();
        }
        state.remove_user(username);
        let state = state.clone();
        self.broadcast(ToClientMsg::SkribblStateChanged(state))
            .await?;
        Ok(())
    }

    async fn on_command_msg(&mut self, _username: &Username, msg: &CommandMsg) -> Result<()> {
        match msg {
            CommandMsg::KickPlayer(kicked_player) => self.remove_player(kicked_player).await?,
        }
        Ok(())
    }

    async fn on_new_message(&mut self, username: Username, msg: data::Message) -> Result<()> {
        let mut should_broadcast = true;
        match self.game_state {
            GameState::Skribbl(ref mut state) => {
                let can_guess = state.can_guess(&username);
                let remaining_time = state.remaining_time();
                let current_word = state.current_word().to_string();
                let noone_already_solved = state
                    .player_states
                    .iter()
                    .all(|(_, player)| !player.has_solved);

                if let Some(player_state) = state.player_states.get_mut(&username) {
                    if can_guess && msg.text().eq_ignore_ascii_case(&current_word) {
                        should_broadcast = false;
                        if noone_already_solved {
                            state.round_end_time -= remaining_time as u64 / 2;
                        }
                        player_state.on_solve(remaining_time);
                        let all_solved = state.did_all_solve();
                        if all_solved {
                            state.next_turn();
                        }
                        let state = state.clone();
                        tokio::try_join!(
                            self.broadcast(ToClientMsg::SkribblStateChanged(state)),
                            self.broadcast_system_msg(format!("{} guessed it!", username)),
                        )?;
                        if all_solved {
                            self.lines.clear();
                            tokio::try_join!(
                                self.broadcast(ToClientMsg::ClearCanvas),
                                self.broadcast_system_msg(format!(
                                    "The word was: \"{}\"",
                                    current_word
                                ))
                            )?;
                        }
                    } else if is_very_close_to(msg.text().to_string(), current_word.to_string()) {
                        should_broadcast = false;
                        if can_guess {
                            self.send_to(
                                &username,
                                ToClientMsg::NewMessage(Message::SystemMsg(
                                    "You're very close!".to_string(),
                                )),
                            )
                            .await?;
                        }
                    }
                }
            }
            GameState::FreeDraw => {
                if let Some(words) = &self.words {
                    let skribbl_state = SkribblState::new(
                        self.sessions.keys().cloned().collect::<Vec<Username>>(),
                        words.clone(),
                    );
                    self.game_state = GameState::Skribbl(skribbl_state.clone());
                    self.broadcast(ToClientMsg::SkribblStateChanged(skribbl_state))
                        .await?;
                }
            }
        }

        if should_broadcast {
            self.broadcast(ToClientMsg::NewMessage(msg)).await?;
        }

        Ok(())
    }

    async fn on_to_srv_msg(&mut self, username: Username, msg: ToServerMsg) -> Result<()> {
        match msg {
            ToServerMsg::CommandMsg(msg) => {
                self.on_command_msg(&username, &msg).await?;
            }
            ToServerMsg::NewMessage(message) => {
                self.on_new_message(username, message).await?;
            }
            ToServerMsg::NewLine(line) => {
                self.lines.push(line);
                self.broadcast(ToClientMsg::NewLine(line)).await?;
            }
            ToServerMsg::ClearCanvas => {
                self.lines.clear();
                self.broadcast(ToClientMsg::ClearCanvas).await?;
            }
        }
        Ok(())
    }

    pub async fn on_tick(&mut self) -> Result<()> {
        let state = match &mut self.game_state {
            GameState::Skribbl(state) => state,
            _ => return Ok(()),
        };

        let remaining_time = state.remaining_time();
        let revealed_char_cnt = state.revealed_characters().len();

        if remaining_time <= 0 {
            let old_word = state.current_word().to_string();
            if let Some(ref mut drawing_user) = state.player_states.get_mut(&state.drawing_user) {
                drawing_user.score += 50;
            }

            state.next_turn();
            let state = self.game_state.skribbl_state().unwrap().clone();
            self.lines.clear();
            tokio::try_join!(
                self.broadcast(ToClientMsg::SkribblStateChanged(state)),
                self.broadcast(ToClientMsg::ClearCanvas),
                self.broadcast_system_msg(format!("The word was: \"{}\"", old_word)),
            )?;
        } else if remaining_time <= (ROUND_DURATION / 4) as u32 && revealed_char_cnt < 2
            || remaining_time <= (ROUND_DURATION / 2) as u32 && revealed_char_cnt < 1
        {
            state.reveal_random_char();
            let state = state.clone();
            self.broadcast(ToClientMsg::SkribblStateChanged(state))
                .await?;
        }

        self.broadcast(ToClientMsg::TimeChanged(remaining_time as u32))
            .await?;

        Ok(())
    }

    pub async fn on_user_joined(&mut self, session: UserSession) -> Result<()> {
        if let GameState::Skribbl(ref mut state) = self.game_state {
            state.add_player(session.username.clone());
            let state = state.clone();
            tokio::try_join!(
                self.broadcast(ToClientMsg::SkribblStateChanged(state)),
                self.broadcast_system_msg(format!("{} joined", session.username)),
            )?;
        }

        let initial_state = InitialState {
            lines: self.lines.clone(),
            skribbl_state: self.game_state.skribbl_state().cloned(),
            dimensions: self.dimensions,
        };
        session
            .send(ToClientMsg::InitialState(initial_state))
            .await?;
        self.sessions.insert(session.username.clone(), session);
        Ok(())
    }

    /// send a Message::SystemMsg to all active sessions
    async fn broadcast_system_msg(&self, msg: String) -> Result<()> {
        self.broadcast(ToClientMsg::NewMessage(Message::SystemMsg(msg)))
            .await?;
        Ok(())
    }

    /// send a ToClientMsg to a specific session
    pub async fn send_to(&self, user: &Username, msg: ToClientMsg) -> Result<()> {
        self.sessions
            .get(user)
            .ok_or(ServerError::UserNotFound(user.to_string()))?
            .send(msg)
            .await?;
        Ok(())
    }

    /// broadcast a ToClientMsg to all running sessions
    async fn broadcast(&self, msg: ToClientMsg) -> Result<()> {
        futures_util::future::try_join_all(
            self.sessions
                .iter()
                .map(|(_, session)| session.send(msg.clone())),
        )
        .await?;
        Ok(())
    }

    /// run the main server, reacting to any server events
    async fn run(&mut self, mut evt_recv: tokio::sync::mpsc::Receiver<ServerEvent>) -> Result<()> {
        loop {
            if let Some(evt) = evt_recv.recv().await {
                match evt {
                    ServerEvent::ToServerMsg(name, msg) => {
                        self.on_to_srv_msg(name.into(), msg).await?
                    }
                    ServerEvent::UserJoined(session) => self.on_user_joined(session).await?,
                    ServerEvent::UserLeft(username) => self.remove_player(&username).await?,
                    ServerEvent::Tick => self.on_tick().await?,
                }
            }
        }
    }
}

pub async fn run_server(
    addr: &str,
    dimensions: (usize, usize),
    word_file: Option<PathBuf>,
) -> Result<()> {
    let mut server_listener = TcpListener::bind(addr)
        .await
        .expect("Could not start webserver (could not bind)");

    let maybe_words = word_file.map(|path| read_words_file(&path).unwrap());

    let (srv_event_send, srv_event_recv) = tokio::sync::mpsc::channel::<ServerEvent>(1);
    let mut server_state = ServerState::new(GameState::FreeDraw, dimensions, maybe_words);

    tokio::spawn(async move {
        server_state.run(srv_event_recv).await.unwrap();
    });

    while let Ok((stream, _)) = server_listener.accept().await {
        let peer = stream.peer_addr().expect("Peer didn't have an address");
        tokio::spawn(handle_connection(peer, stream, srv_event_send.clone()));
    }
    Ok(())
}

async fn handle_connection(
    peer: SocketAddr,
    stream: TcpStream,
    mut srv_event_send: tokio::sync::mpsc::Sender<ServerEvent>,
) -> Result<()> {
    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    println!("new WebSocket connection: {}", peer);
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // first, wait for the client to send his username
    let username: Username = loop {
        let msg = ws_receiver
            .next()
            .await
            .expect("No username message received")?;
        if let tungstenite::Message::Text(username) = msg {
            break username.into();
        }
    };

    let (session_msg_send, mut session_msg_recv) = tokio::sync::mpsc::channel(1);
    let (session_close_send, mut session_close_recv) = tokio::sync::mpsc::channel(1);

    // then, create a session and send that session to the server's main thread
    let session = UserSession::new(username.clone(), session_msg_send, session_close_send);
    srv_event_send
        .send(ServerEvent::UserJoined(session))
        .await?;

    // TODO look at stream forwarding for this...
    // asynchronously read messages that the main server thread wants
    // to send to this client and forward them to the WS client
    let send_thread = tokio::spawn(async move {
        loop {
            tokio::select! {
                maybe_msg = session_msg_recv.recv() => match maybe_msg {
                    Some(msg) => {
                        let msg = serde_json::to_string(&msg).expect("Could not serialize msg");
                        let result = ws_sender.send(tungstenite::Message::Text(msg)).await;
                        if let Err(_) = result {
                            break result;
                        }
                    }
                    // if the msg received is None, all senders have been closed, so we can finish
                    None => {
                        ws_sender.send(tungstenite::Message::Close(None)).await?;
                        break Ok(());
                    }
                },
                _ = session_close_recv.recv() => {
                    ws_sender.send(tungstenite::Message::Close(None)).await?;
                    break Ok(());
                }
            }
        }
    });

    // TODO look at stream forwarding for this
    // forward other events to the main server thread
    loop {
        let delay = Delay::new(Duration::from_millis(500));
        tokio::select! {
            // every 100ms, send a tick event to the main server thread
            _ = delay => srv_event_send.send(ServerEvent::Tick).await?,

            // Websocket messages from the client
            msg = ws_receiver.next() => match msg {
                Some(Ok(tungstenite::Message::Text(msg))) => match serde_json::from_str(&msg) {
                    Ok(Some(msg)) => {
                        srv_event_send
                            .send(ServerEvent::ToServerMsg(username.clone(), msg))
                            .await?;
                    }
                    Ok(None) => {
                        break;
                    }
                    Err(err) => {
                        eprintln!("{} (msg was: {})", err, msg);
                    }
                },
                Some(Ok(tungstenite::Message::Close(_))) | Some(Err(_)) | None => break,
                _ => {}
            }
        }
    }

    drop(send_thread);
    srv_event_send.send(ServerEvent::UserLeft(username)).await?;
    Ok(())
}

pub fn read_words_file(path: &PathBuf) -> Result<Vec<String>> {
    let mut file = std::fs::File::open(path)?;
    let mut words = String::new();
    file.read_to_string(&mut words)?;
    Ok(words
        .lines()
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect::<Vec<String>>())
}

fn is_very_close_to(a: String, b: String) -> bool {
    return levenshtein_distance(a, b) <= 1;
}

fn levenshtein_distance(a: String, b: String) -> usize {
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
