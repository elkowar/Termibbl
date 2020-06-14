use std::{
    io::{stdout, Write},
    sync::mpsc,
    thread,
};

use app::App;
use crossterm::{
    event::{read, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, MouseEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    Result,
};

use server::ToClientMsg;
use tui::{backend::CrosstermBackend, Terminal};

pub mod app;
pub mod data;
pub mod server;
pub mod ui;

pub use serde::{Deserialize, Serialize};

pub const CANVAS_SIZE: (usize, usize) = (100, 50);
pub const PALETTE_SIZE: usize = 100;

#[tokio::main]
async fn main() -> Result<()> {
    match std::env::args().nth(1) {
        Some(arg) => {
            server::run_server().await;
            Ok(())
        }
        _ => run_client().await,
    }
}

pub enum ClientEvent {
    MouseInput(MouseEvent),
    KeyInput(KeyEvent),
    ServerMessage(ToClientMsg),
}

async fn run_client() -> Result<()> {
    let (send, recv) = mpsc::channel::<ClientEvent>();
    let (websocket_send, websocket_recv) = mpsc::channel::<ToClientMsg>();
    let mut app = App::new();
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    execute!(stdout(), EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    thread::spawn(move || app.run(&mut terminal, recv));
    tokio::spawn(async move {
        let mut ws: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream> =
            tokio_tungstenite::connect_async("ws://localhost:8080")
                .await
                .unwrap()
                .0;
        //ws.write_all(b"hi").await.unwrap();
        //loop {
        //match ws.0.read_message() {
        //Ok(msg) => println!("{}", msg),
        //_ => {}
        //}
        //}
    });

    loop {
        match read()? {
            Event::Key(evt) => match evt {
                KeyEvent {
                    code: KeyCode::Esc,
                    modifiers: _,
                } => break,
                _ => send.send(ClientEvent::KeyInput(evt)).unwrap(),
            },
            Event::Mouse(evt) => send.send(ClientEvent::MouseInput(evt)).unwrap(),
            _ => {}
        }
    }

    execute!(stdout(), DisableMouseCapture)?;
    execute!(stdout(), LeaveAlternateScreen)?;
    disable_raw_mode()
}
