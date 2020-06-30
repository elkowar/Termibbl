//https://github.com/snapview/tokio-tungstenite/blob/master/examples/server.rs

use super::skribbl::{get_time_now, SkribblState};
use crate::{
    data,
    message::{InitialState, ToClientMsg, ToServerMsg},
};
use data::{Message, Username};
use futures_timer::Delay;
use futures_util::{SinkExt, StreamExt};
use rand::seq::SliceRandom;
use std::io::Read;
use std::net::SocketAddr;
use std::{collections::HashMap, path::PathBuf, time::Duration};
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
}

impl UserSession {
    fn new(username: Username, msg_send: tokio::sync::mpsc::Sender<ToClientMsg>) -> Self {
        UserSession {
            username,
            msg_send: Mutex::new(msg_send),
        }
    }

    async fn send(&self, msg: ToClientMsg) -> Result<()> {
        self.msg_send.lock().await.send(msg.clone()).await?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum GameState {
    FreeDraw,
    Skribbl(Vec<String>, Option<SkribblState>),
}

impl GameState {
    fn skribbl_state(&self) -> Option<&SkribblState> {
        match self {
            GameState::Skribbl(_, Some(state)) => Some(state),
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
}

impl ServerState {
    fn new(game_state: GameState, dimensions: (usize, usize)) -> Self {
        ServerState {
            sessions: HashMap::new(),
            lines: Vec::new(),
            dimensions,
            game_state,
        }
    }

    async fn on_new_message(&mut self, username: Username, msg: data::Message) -> Result<()> {
        let mut did_solve = false;
        match self.game_state {
            GameState::Skribbl(ref mut words, Some(ref mut state)) => {
                let can_guess = state.can_guess(&username);
                let current_word = &state.current_word;

                if msg.text().starts_with("!kick ") {
                    state.player_states.remove(&Username::from(
                        msg.text().trim_start_matches("!kick ").to_string(),
                    ));

                    let state = state.clone();
                    self.broadcast(ToClientMsg::SkribblStateChanged(state))
                        .await?;
                    return Ok(());
                }

                if let Some(player_state) = state.player_states.get_mut(&username) {
                    if can_guess && msg.text() == current_word {
                        player_state.on_solve();
                        did_solve = true;
                        if state.did_all_solve() {
                            *words = words
                                .into_iter()
                                .filter(|x| x != &&state.current_word)
                                .map(|x| x.clone())
                                .collect();
                            state.next_player(
                                words.choose(&mut rand::thread_rng()).unwrap().clone(),
                            );
                        }
                        let state = state.clone();
                        let solved_msg = Message::SystemMsg(format!("{} solved it!", username));
                        self.broadcast(ToClientMsg::NewMessage(solved_msg)).await?;
                        self.broadcast(ToClientMsg::SkribblStateChanged(state))
                            .await?;
                    }
                }
            }
            GameState::Skribbl(ref words, None) => {
                let skribbl_state =
                    SkribblState::with_users(self.sessions.keys().cloned().collect(), &words);
                self.game_state = GameState::Skribbl(words.clone(), Some(skribbl_state.clone()));
                self.broadcast(ToClientMsg::SkribblStateChanged(skribbl_state))
                    .await?;
            }
            GameState::FreeDraw => {}
        }

        if !did_solve {
            self.broadcast(ToClientMsg::NewMessage(msg)).await?;
        }

        Ok(())
    }

    async fn on_to_srv_msg(&mut self, username: Username, msg: ToServerMsg) -> Result<()> {
        match msg {
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
        match self.game_state {
            GameState::Skribbl(ref mut words, Some(ref mut state)) => {
                let elapsed_time = get_time_now() - state.round_start_time;
                if elapsed_time > ROUND_DURATION {
                    *words = words
                        .into_iter()
                        .filter(|x| x != &&state.current_word)
                        .map(|x| x.clone())
                        .collect();
                    state.next_player(words.choose(&mut rand::thread_rng()).unwrap().clone());
                    let state = state.clone();
                    self.broadcast(ToClientMsg::SkribblStateChanged(state))
                        .await?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub async fn on_user_joined(&mut self, session: UserSession) -> Result<()> {
        match self.game_state {
            GameState::Skribbl(_, Some(ref mut state)) => {
                state.add_player(session.username.clone());
                let state = state.clone();
                self.broadcast(ToClientMsg::SkribblStateChanged(state))
                    .await?;

                let joined_msg = data::Message::SystemMsg(format!("{} joined", session.username));
                self.broadcast(ToClientMsg::NewMessage(joined_msg)).await?;
            }
            _ => {}
        }

        session
            .send(ToClientMsg::InitialState(InitialState {
                lines: self.lines.clone(),
                skribbl_state: self.game_state.skribbl_state().cloned(),
                dimensions: self.dimensions,
            }))
            .await?;
        self.sessions.insert(session.username.clone(), session);
        Ok(())
    }

    pub async fn on_user_leave(&mut self, username: &Username) {
        println!("user left: {}", username);
        self.sessions.remove(username);
    }

    #[allow(unused)]
    pub async fn send_to(&self, user: &Username, msg: ToClientMsg) -> Result<()> {
        self.sessions
            .get(user)
            .ok_or(ServerError::UserNotFound(user.to_string()))?
            .send(msg)
            .await?;
        Ok(())
    }

    async fn broadcast(&self, msg: ToClientMsg) -> Result<()> {
        for (_, session) in self.sessions.iter() {
            session.send(msg.clone()).await?;
        }
        Ok(())
    }

    async fn run(&mut self, mut evt_recv: tokio::sync::mpsc::Receiver<ServerEvent>) -> Result<()> {
        loop {
            if let Some(evt) = evt_recv.recv().await {
                match evt {
                    ServerEvent::ToServerMsg(name, msg) => {
                        self.on_to_srv_msg(name.into(), msg).await?
                    }
                    ServerEvent::UserJoined(session) => self.on_user_joined(session).await?,
                    ServerEvent::UserLeft(username) => self.on_user_leave(&username).await,
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
    println!("Running server on {}", addr);
    let mut server_listener = TcpListener::bind(addr)
        .await
        .expect("Could not start webserver (could not bind)");

    let game_state = if let Some(word_file) = word_file {
        let mut file = std::fs::File::open(word_file)?;
        let mut words = String::new();
        file.read_to_string(&mut words)?;
        let words = words
            .lines()
            .map(|x| x.trim().to_string())
            .collect::<Vec<String>>();
        GameState::Skribbl(words, None)
    } else {
        GameState::FreeDraw
    };

    let (srv_event_send, srv_event_recv) = tokio::sync::mpsc::channel::<ServerEvent>(1);
    let mut server_state = ServerState::new(game_state, dimensions);

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

    srv_event_send
        .send(ServerEvent::UserJoined(UserSession::new(
            username.clone(),
            session_msg_send,
        )))
        .await?;

    let send_thread = tokio::spawn(async move {
        loop {
            if let Some(Ok(msg)) = session_msg_recv
                .recv()
                .await
                .map(|msg| serde_json::to_string(&msg))
            {
                let result = ws_sender.send(tungstenite::Message::Text(msg)).await;
                if let Err(_) = result {
                    return result;
                }
            }
        }
    });

    loop {
        let delay = Delay::new(Duration::from_millis(100));
        tokio::select! {
            _ = delay => srv_event_send.send(ServerEvent::Tick).await?,
            msg = ws_receiver.next() => match msg {
                Some(Ok(tungstenite::Message::Text(msg))) => match serde_json::from_str(&msg) {
                    Ok(Some(msg)) => {
                        srv_event_send
                            .send(ServerEvent::ToServerMsg(username.clone(), msg))
                            .await?;
                    }
                    Ok(None) => {
                        panic!("Got none. TODO: cannot be bothered to handle this correctly rn");
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
