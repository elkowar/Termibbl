//https://github.com/snapview/tokio-tungstenite/blob/master/examples/server.rs

use crate::{
    data,
    message::{InitialState, ToClientMsg, ToServerMsg},
};
use data::SkribblState;
use futures_util::{SinkExt, StreamExt};
use std::io::Read;
use std::net::SocketAddr;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
};

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

#[derive(Debug, Clone)]
struct UserSession {
    username: String,
    points: i32,
    msg_send: tokio::sync::mpsc::Sender<ToClientMsg>,
}

impl UserSession {
    fn new(username: String, msg_send: tokio::sync::mpsc::Sender<ToClientMsg>) -> Self {
        UserSession {
            username,
            msg_send,
            points: 0,
        }
    }

    async fn send(&mut self, msg: ToClientMsg) -> Result<()> {
        self.msg_send.send(msg.clone()).await?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum GameState {
    FreeDraw,
    Skribbl(Vec<String>, Option<SkribblState>),
}

#[derive(Debug)]
struct ServerState {
    sessions: HashMap<String, UserSession>,
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
    async fn on_message(&mut self, _username: &str, msg: ToServerMsg) -> Result<()> {
        match msg {
            ToServerMsg::NewMessage(message) => {
                println!("new message: {:?}", message);
                self.broadcast(ToClientMsg::NewMessage(message.clone()))
                    .await?;

                match &self.game_state {
                    GameState::FreeDraw => {}
                    GameState::Skribbl(_words, Some(state))
                        if message.text == state.current_word => {}
                    _ => {}
                }
            }
            ToServerMsg::NewLine(line) => {
                self.lines.push(line);
                self.broadcast(ToClientMsg::NewLine(line)).await?;
            }
        }
        Ok(())
    }

    pub async fn on_user_joined(&mut self, mut session: UserSession) -> Result<()> {
        session
            .send(ToClientMsg::InitialState(InitialState {
                lines: self.lines.clone(),
                current_users: self
                    .sessions
                    .keys()
                    .map(|x| x.to_string())
                    .collect::<Vec<String>>(),
                dimensions: self.dimensions,
            }))
            .await?;
        self.broadcast(ToClientMsg::UserJoined(session.username.clone()))
            .await?;
        self.sessions.insert(session.username.clone(), session);
        Ok(())
    }
    pub async fn on_user_leave(&mut self, username: &str) {
        println!("user left: {}", username);
        self.sessions.remove(username);
    }

    #[allow(unused)]
    pub async fn send_to(&mut self, user: &str, msg: ToClientMsg) -> Result<()> {
        self.sessions
            .get_mut(user)
            .ok_or(ServerError::UserNotFound(user.to_string()))?
            .send(msg)
            .await?;
        Ok(())
    }

    async fn broadcast(&mut self, msg: ToClientMsg) -> Result<()> {
        for (_, session) in self.sessions.iter_mut() {
            session.send(msg.clone()).await?;
        }
        Ok(())
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
            .map(|x| x.to_string())
            .collect::<Vec<String>>();
        GameState::Skribbl(words, None)
    } else {
        GameState::FreeDraw
    };

    let server_state = Arc::new(Mutex::new(ServerState::new(game_state, dimensions)));

    while let Ok((stream, _)) = server_listener.accept().await {
        let peer = stream.peer_addr().expect("Peer didn't have an address");
        tokio::spawn(handle_connection(peer, stream, server_state.clone()));
    }
    Ok(())
}

async fn handle_connection(
    peer: SocketAddr,
    stream: TcpStream,
    state: Arc<Mutex<ServerState>>,
) -> Result<()> {
    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    println!("new WebSocket connection: {}", peer);
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    let username = loop {
        let msg = ws_receiver
            .next()
            .await
            .expect("No username message received")?;
        if let tungstenite::Message::Text(username) = msg {
            break username;
        }
    };

    let (session_msg_send, mut session_msg_recv) = tokio::sync::mpsc::channel(1);

    state
        .lock()
        .await
        .on_user_joined(UserSession::new(username.clone(), session_msg_send))
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
        let msg = ws_receiver.next().await;
        let mut state = state.lock().await;
        match msg {
            Some(Ok(tungstenite::Message::Text(msg))) => match serde_json::from_str(&msg) {
                Ok(Some(msg)) => {
                    state.on_message(&username, msg).await?;
                }
                Ok(None) => {
                    println!("got none");
                }
                Err(err) => {
                    println!("{} (msg was: {})", err, msg);
                }
            },
            Some(Ok(tungstenite::Message::Close(_))) | Some(Err(_)) | None => break,
            _ => {}
        }
    }

    drop(send_thread);
    state.lock().await.on_user_leave(&username).await;
    Ok(())
}
