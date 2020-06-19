use std::{
    io::{stdout, Write},
    sync::mpsc,
    thread,
};
use tokio::prelude::*;

use app::App;
use crossterm::{
    event::{read, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, MouseEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    Result,
};

use server::{ToClientMsg, ToServerMsg};
use tui::{backend::CrosstermBackend, Terminal};

pub mod app;
pub mod data;
pub mod server;
pub mod ui;

pub use serde::{Deserialize, Serialize};
use tokio_tungstenite;

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
    let (client_evt_send, client_evt_recv) = mpsc::channel::<ClientEvent>();
    let (to_server_send, to_server_recv) = tokio::sync::mpsc::unbounded_channel::<ToServerMsg>();

    let mut app = App::new();
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    execute!(stdout(), EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    thread::spawn(move || app.run(&mut terminal, client_evt_recv));
    let mut ws = tokio_tungstenite::connect_async("ws://localhost:8080")
        .await
        .unwrap()
        .0;
    let (ws_read, ws_write) = ws.get_mut().split();
    tokio::spawn(async {
        tokio::select! {
            to_server_msg = to_server_recv.recv() => {
                ws_write.write_all(
                    serde_json::to_string(&to_server_msg)
                        .unwrap()
                        .as_bytes(),
                );
            },
        }
    });
    tokio::spawn(async move {
        ws_write.write_all(b"hi");
        loop {
            let mut msg = String::new();
            ws_read.read_to_string(&mut msg);
            let msg = serde_json::from_str(&msg).unwrap();
            client_evt_send
                .send(ClientEvent::ServerMessage(msg))
                .unwrap();
        }
    });

    loop {
        match read()? {
            Event::Key(evt) => match evt {
                KeyEvent {
                    code: KeyCode::Esc,
                    modifiers: _,
                } => break,
                _ => client_evt_send.send(ClientEvent::KeyInput(evt)).unwrap(),
            },
            Event::Mouse(evt) => client_evt_send.send(ClientEvent::MouseInput(evt)).unwrap(),
            _ => {}
        }
    }

    execute!(stdout(), DisableMouseCapture)?;
    execute!(stdout(), LeaveAlternateScreen)?;
    disable_raw_mode()
}
