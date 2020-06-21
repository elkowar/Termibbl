use crate::{
    client::error::Result,
    client::ui,
    data::{AppLine, CanvasColor, Coord, Message},
    message::{ToClientMsg, ToServerMsg},
    ClientEvent, CANVAS_SIZE,
};
use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;

use tokio_tungstenite::WebSocketStream;
use tui::{backend::Backend, style::Color, Terminal};

#[derive(Debug, Clone)]
pub struct AppCanvas {
    pub palette: Vec<CanvasColor>,
    pub lines: Vec<AppLine>,
    pub last_mouse_pos: Option<Coord>,
    pub current_color: CanvasColor,
}

impl AppCanvas {
    fn new() -> Self {
        AppCanvas {
            lines: Vec::new(),
            last_mouse_pos: None,
            current_color: CanvasColor(Color::White),
            palette: [
                CanvasColor(Color::White),
                CanvasColor(Color::Gray),
                CanvasColor(Color::DarkGray),
                CanvasColor(Color::Black),
                CanvasColor(Color::Red),
                CanvasColor(Color::LightRed),
                CanvasColor(Color::Green),
                CanvasColor(Color::LightGreen),
                CanvasColor(Color::Blue),
                CanvasColor(Color::LightBlue),
                CanvasColor(Color::Yellow),
                CanvasColor(Color::LightYellow),
                CanvasColor(Color::Cyan),
                CanvasColor(Color::LightCyan),
                CanvasColor(Color::Magenta),
                CanvasColor(Color::LightMagenta),
            ]
            .to_vec(),
        }
    }
}

impl AppCanvas {
    pub fn apply_mouse_event(&mut self, evt: MouseEvent) -> Option<()> {
        match evt {
            MouseEvent::Down(_, x, y, _) => {
                if y == 0 {
                    let swatch_size = CANVAS_SIZE.0 / self.palette.len() as usize;
                    let selected_color = self.palette.get(x as usize / swatch_size);
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
                self.lines.push(AppLine::new(
                    self.last_mouse_pos.unwrap_or(mouse_pos),
                    mouse_pos,
                    self.current_color,
                ));
                self.last_mouse_pos = Some(mouse_pos);
            }
            _ => {}
        }
        Some(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct Chat {
    pub input: String,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone)]
pub struct App {
    pub canvas: AppCanvas,
    pub chat: Chat,
    pub session: ServerSession,
}

impl App {
    pub fn new(session: ServerSession) -> App {
        App {
            canvas: AppCanvas::new(),
            chat: Chat::default(),
            session,
        }
    }

    pub async fn handle_chat_key_event(&mut self, code: &KeyCode) -> Result<()> {
        match code {
            KeyCode::Char(c) => {
                self.chat.input.push(*c);
            }
            KeyCode::Enter => {
                let message = Message::new(self.session.username.clone(), self.chat.input.clone());
                self.session.send(ToServerMsg::NewMessage(message)).await?;
                self.chat.input = String::new();
            }
            KeyCode::Backspace => {
                self.chat.input.pop();
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
                self.canvas.apply_mouse_event(mouse_evt);
            }
            ClientEvent::ServerMessage(m) => match m {
                ToClientMsg::NewMessage(message) => self.chat.messages.push(message),
                _ => {}
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
            ui::draw(self, &mut terminal);
            let event = chan.recv().await;
            if let Some(event) = event {
                self.handle_event(event).await?;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServerSession {
    to_server_send: tokio::sync::mpsc::Sender<ToServerMsg>,
    username: String,
}

impl ServerSession {
    pub async fn establish_connection(
        username: String,
        mut evt_send: tokio::sync::mpsc::Sender<ClientEvent>,
    ) -> Result<ServerSession> {
        let (to_server_send, mut to_server_recv) = tokio::sync::mpsc::channel::<ToServerMsg>(1);

        let ws: WebSocketStream<_> = tokio_tungstenite::connect_async("ws://localhost:8080")
            .await
            .expect("Could not connect to server")
            .0;
        let (mut ws_send, mut ws_recv) = ws.split();

        ws_send
            .send(tungstenite::Message::Text(username.clone()))
            .await
            .unwrap();

        tokio::spawn(async move {
            loop {
                let msg = to_server_recv.recv().await;
                let msg = serde_json::to_string(&msg).unwrap();
                if let Err(_) = ws_send.send(tungstenite::Message::Text(msg)).await {
                    break;
                }
            }
        });

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
        });
        Ok(ServerSession {
            to_server_send,
            username,
        })
    }

    pub async fn send(&mut self, message: ToServerMsg) -> Result<()> {
        self.to_server_send.send(message).await?;
        Ok(())
    }
}
