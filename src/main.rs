pub mod client;
pub mod data;
pub mod message;
pub mod server;

use std::io::{stdout, Write};
use std::path::PathBuf;
use structopt::StructOpt;

use crossterm::{
    event::{read, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, MouseEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    Result,
};

use tui::{backend::CrosstermBackend, Terminal};

use client::app::ServerSession;
use data::Username;
pub use serde::{Deserialize, Serialize};

#[derive(Debug, StructOpt)]
#[structopt(name = "Termibbl", about = "A Skribbl.io-alike for the terminal")]
struct Opt {
    #[structopt(subcommand)]
    cmd: SubOpt,
}

#[derive(Debug, StructOpt)]
enum SubOpt {
    Server {
        #[structopt(long = "--port", short = "-p")]
        port: u32,
        #[structopt(long = "--words", parse(from_os_str), required_if("freedraw", "true"))]
        word_file: Option<PathBuf>,
        #[structopt(short, long, help = "<width>x<height>", parse(from_str = crate::parse_dimension), default_value = "100x50")]
        dimensions: (usize, usize),
    },
    Client {
        #[structopt(long = "address", short = "-a")]
        addr: String,
        username: String,
    },
}

fn parse_dimension(s: &str) -> (usize, usize) {
    let mut split = s.split('x');
    (
        split.next().unwrap().parse().unwrap(),
        split.next().unwrap().parse().unwrap(),
    )
}

#[tokio::main]
async fn main() -> Result<()> {
    let opt = Opt::from_args();
    match opt.cmd {
        SubOpt::Client { username, addr } => {
            let addr = if addr.starts_with("ws://") || addr.starts_with("wss://") {
                addr
            } else {
                format!("ws://{}", addr)
            };
            run_client(&addr, username.into()).await.unwrap();
        }
        SubOpt::Server {
            port,
            word_file,
            dimensions,
        } => {
            tokio::spawn(async move {
                if let Ok(res) = reqwest::get("http://ifconfig.me").await {
                    if let Ok(ip) = res.text().await {
                        println!("Starting server!");
                        println!("Your public IP is {}:{}", ip, port);
                        println!("You can find out your private IP by running \"ifconfig\" in the terminal");
                    }
                }
            });

            let addr = format!("0.0.0.0:{}", port);
            server::server::run_server(&addr, dimensions, word_file)
                .await
                .unwrap();
        }
    }
    Ok(())
}

pub enum ClientEvent {
    MouseInput(MouseEvent),
    KeyInput(KeyEvent),
    ServerMessage(message::ToClientMsg),
}

async fn run_client(addr: &str, username: Username) -> client::error::Result<()> {
    let (mut client_evt_send, client_evt_recv) = tokio::sync::mpsc::channel::<ClientEvent>(1);

    let mut app =
        ServerSession::establish_connection(addr, username, client_evt_send.clone()).await?;

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    execute!(stdout(), EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    tokio::spawn(async move {
        app.run(&mut terminal, client_evt_recv).await.unwrap();
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
    disable_raw_mode()?;
    Ok(())
}
