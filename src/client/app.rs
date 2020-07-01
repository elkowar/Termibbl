use crate::{
    client::error::Result,
    client::ui,
    data::{self, CanvasColor, Coord, Line, Message},
    message::{InitialState, ToClientMsg, ToServerMsg},
    server::skribbl::SkribblState,
    ClientEvent,
};
use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;

use data::{CommandMsg, Username};
use tokio_tungstenite::WebSocketStream;
use tui::{backend::Backend, Terminal};

const PALETTE: [CanvasColor; 16] = [
    CanvasColor::White,
    CanvasColor::Gray,
    CanvasColor::DarkGray,
    CanvasColor::Black,
    CanvasColor::Red,
    CanvasColor::LightRed,
    CanvasColor::Green,
    CanvasColor::LightGreen,
    CanvasColor::Blue,
    CanvasColor::LightBlue,
    CanvasColor::Yellow,
    CanvasColor::LightYellow,
    CanvasColor::Cyan,
    CanvasColor::LightCyan,
    CanvasColor::Magenta,
    CanvasColor::LightMagenta,
];

#[derive(Debug, Clone)]
pub struct AppCanvas {
    pub palette: Vec<CanvasColor>,
    pub lines: Vec<data::Line>,
    pub dimensions: (usize, usize),
}

impl AppCanvas {
    fn new(dimensions: (usize, usize), lines: Vec<data::Line>) -> Self {
        AppCanvas {
            lines,
            dimensions,
            palette: PALETTE.to_vec(),
        }
    }
}

impl AppCanvas {
    pub fn draw_line(&mut self, line: Line) {
        self.lines.push(line);
    }
}

#[derive(Debug, Clone, Default)]
pub struct Chat {
    pub input: String,
    pub messages: Vec<Message>,
}

#[derive(Debug)]
pub struct App {
    pub canvas: AppCanvas,
    pub chat: Chat,
    pub session: ServerSession,
    pub last_mouse_pos: Option<Coord>,
    pub current_color: CanvasColor,
    pub game_state: Option<SkribblState>,
    pub remaining_time: Option<u32>,
}

impl App {
    pub fn new(session: ServerSession, initial_state: InitialState) -> App {
        App {
            canvas: AppCanvas::new(initial_state.dimensions, initial_state.lines),
            chat: Chat::default(),
            last_mouse_pos: None,
            current_color: CanvasColor::White,
            game_state: initial_state.skribbl_state,
            session,
            remaining_time: None,
        }
    }

    pub fn is_drawing(&self) -> bool {
        self.game_state
            .as_ref()
            .map(|x| x.is_drawing(&self.session.username))
            .unwrap_or(true)
    }

    pub async fn handle_mouse_event(&mut self, evt: MouseEvent) -> Result<()> {
        if !self.is_drawing() {
            return Ok(());
        }

        let dimensions = self.canvas.dimensions;
        match evt {
            MouseEvent::Down(_, x, y, _) => {
                if y == 0 {
                    let swatch_size = dimensions.0 / self.canvas.palette.len() as usize;
                    let selected_color = self.canvas.palette.get(x as usize / swatch_size);
                    match selected_color {
                        Some(color) => self.current_color = color.clone(),
                        _ => {}
                    }
                } else {
                    self.last_mouse_pos = Some(Coord(x, y));
                }
            }
            MouseEvent::Up(_, _, _, _) => {
                self.last_mouse_pos = None;
            }
            MouseEvent::Drag(_, x, y, _) => {
                let mouse_pos = Coord(x, y);
                let line = Line::new(
                    self.last_mouse_pos.unwrap_or(mouse_pos),
                    mouse_pos,
                    self.current_color,
                );
                self.canvas.draw_line(line);
                self.session.send(ToServerMsg::NewLine(line)).await?;
                self.last_mouse_pos = Some(mouse_pos);
            }
            _ => {}
        }
        Ok(())
    }

    pub async fn handle_chat_key_event(&mut self, code: &KeyCode) -> Result<()> {
        match code {
            KeyCode::Char(c) => {
                self.chat.input.push(*c);
            }
            KeyCode::Enter => {
                if self.chat.input.trim().is_empty() {
                    self.chat.input = String::new();
                    return Ok(());
                }

                let msg_content = self.chat.input.clone();
                if msg_content.starts_with("!") {
                    if msg_content.starts_with("!kick ") {
                        let msg_without_cmd =
                            msg_content.trim_start_matches("!kick ").trim().to_string();
                        let command = CommandMsg::KickPlayer(Username::from(msg_without_cmd));
                        self.session.send(ToServerMsg::CommandMsg(command)).await?;
                    };
                } else {
                    let message =
                        Message::UserMsg(self.session.username.clone(), self.chat.input.clone());
                    self.session.send(ToServerMsg::NewMessage(message)).await?;
                }
                self.chat.input = String::new();
            }
            KeyCode::Backspace => {
                self.chat.input.pop();
            }
            KeyCode::Delete => {
                if self.is_drawing() {
                    self.session.send(ToServerMsg::ClearCanvas).await?;
                    self.canvas.lines.clear();
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub async fn handle_event(&mut self, evt: ClientEvent) -> Result<()> {
        match evt {
            ClientEvent::KeyInput(KeyEvent { code, .. }) => {
                self.handle_chat_key_event(&code).await?;
            }
            ClientEvent::MouseInput(mouse_evt) => {
                self.handle_mouse_event(mouse_evt).await?;
            }
            ClientEvent::ServerMessage(m) => match m {
                ToClientMsg::TimeChanged(new_time) => {
                    self.remaining_time = Some(new_time);
                }
                ToClientMsg::NewMessage(message) => self.chat.messages.push(message),
                ToClientMsg::NewLine(line) => {
                    self.canvas.draw_line(line);
                }
                ToClientMsg::SkribblStateChanged(new_state) => {
                    self.game_state = Some(new_state);
                }
                ToClientMsg::ClearCanvas => {
                    self.canvas.lines.clear();
                }
                ToClientMsg::GameOver(state) => {
                    dbg!(state);
                    panic!("Game over, I couldn't yet be bothered to implement this in a better way yet,...");
                }
                ToClientMsg::InitialState(_) => {}
            },
        }
        Ok(())
    }

    pub async fn run<B: Backend>(
        &mut self,
        mut terminal: &mut Terminal<B>,
        mut chan: tokio::sync::mpsc::Receiver<ClientEvent>,
    ) -> Result<()> {
        loop {
            ui::draw(self, &mut terminal)?;
            if let Some(event) = chan.recv().await {
                self.handle_event(event).await?;
            } else {
                break Ok(());
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServerSession {
    to_server_send: tokio::sync::mpsc::Sender<ToServerMsg>,
    pub username: Username,
}

impl ServerSession {
    pub async fn establish_connection(
        addr: &str,
        username: Username,
        mut evt_send: tokio::sync::mpsc::Sender<ClientEvent>,
    ) -> Result<App> {
        let (to_server_send, mut to_server_recv) = tokio::sync::mpsc::channel::<ToServerMsg>(1);

        let ws: WebSocketStream<_> = tokio_tungstenite::connect_async(addr)
            .await
            .expect("Could not connect to server")
            .0;
        let (mut ws_send, mut ws_recv) = ws.split();

        // first send the username to the server
        ws_send
            .send(tungstenite::Message::Text(username.clone().into()))
            .await
            .unwrap();

        // and wait for the initial state
        let initial_state: InitialState = loop {
            let msg = ws_recv.next().await;
            if let Some(Ok(tungstenite::Message::Text(msg))) = msg {
                if let Ok(ToClientMsg::InitialState(state)) = serde_json::from_str(&msg) {
                    break state;
                }
            }
        };

        // forward events to the server
        let send_handle = tokio::spawn(async move {
            loop {
                let msg = to_server_recv.recv().await;
                let msg = serde_json::to_string(&msg).unwrap();
                if let Err(_) = ws_send.send(tungstenite::Message::Text(msg)).await {
                    break;
                }
            }
        });

        // and receive messages from the server
        tokio::spawn(async move {
            loop {
                match ws_recv.next().await {
                    Some(Ok(tungstenite::Message::Text(msg))) => {
                        let msg = serde_json::from_str(&msg).unwrap();
                        let _ = evt_send.send(ClientEvent::ServerMessage(msg)).await;
                    }
                    Some(Ok(tungstenite::Message::Close(_))) => {
                        break;
                    }
                    _ => {}
                }
            }
            std::mem::drop(send_handle);
        });

        Ok(App::new(
            ServerSession {
                to_server_send,
                username,
            },
            initial_state,
        ))
    }

    pub async fn send(&mut self, message: ToServerMsg) -> Result<()> {
        self.to_server_send.send(message).await?;
        Ok(())
    }
}
