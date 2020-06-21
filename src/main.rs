pub mod client;
pub mod data;
pub mod message;
pub mod server;

use std::{
    io::{stdout, Write},
    sync::mpsc,
    thread,
};
use tokio::prelude::*;

use crossterm::{
    event::{read, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, MouseEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    Result,
};

use tui::{backend::CrosstermBackend, Terminal};

use client::app::ServerSession;
pub use serde::{Deserialize, Serialize};
use tokio_tungstenite;

pub const CANVAS_SIZE: (usize, usize) = (100, 50);
pub const PALETTE_SIZE: usize = 100;

#[tokio::main]
async fn main() -> Result<()> {
    match std::env::args().nth(1) {
        Some(arg) => {
            if arg == "--server".to_string() {
                server::server::run_server().await;
            } else {
                run_client(arg).await;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub enum ClientEvent {
    MouseInput(MouseEvent),
    KeyInput(KeyEvent),
    ServerMessage(message::ToClientMsg),
}

async fn run_client(username: String) -> Result<()> {
    let (mut client_evt_send, client_evt_recv) = tokio::sync::mpsc::channel::<ClientEvent>(1);

    let session = ServerSession::establish_connection(username, client_evt_send.clone()).await;

    let mut app = client::app::App::new(session.unwrap());
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    execute!(stdout(), EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    tokio::spawn(async move {
        app.run(&mut terminal, client_evt_recv).await;
    });
    loop {
        match read()? {
            Event::Key(evt) => match evt {
                KeyEvent {
                    code: KeyCode::Esc,
                    modifiers: _,
                } => break,
                _ => {
                    let _ = client_evt_send.send(ClientEvent::KeyInput(evt)).await;
                }
            },
            Event::Mouse(evt) => {
                let _ = client_evt_send.send(ClientEvent::MouseInput(evt)).await;
            }
            _ => {}
        }
    }

    execute!(stdout(), DisableMouseCapture)?;
    execute!(stdout(), LeaveAlternateScreen)?;
    disable_raw_mode()
}
