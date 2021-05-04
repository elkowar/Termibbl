#![allow(dead_code, unused_variables)]
mod client;
mod data;
mod encoding;
mod events;
mod message;
mod server;
mod utils;

use client::App;
use data::GameOpts;
use events::EventSender;
use server::GameServer;
use utils::dispatch_abortable_task;

use argh::FromArgs;

use std::{error::Error, net::SocketAddr};

/// A Skribbl.io-alike for the terminal
#[derive(FromArgs)]
struct Opt {
    #[argh(subcommand)]
    cmd: Option<SubOpt>,

    #[argh(switch, description = "write out debug logs.")]
    log_debug: bool,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum SubOpt {
    Client(client::CliOpts),
    Server(server::CliOpts),
}

async fn process_ctrl_c(tx: EventSender<server::Message>) {
    let _ = tokio::signal::ctrl_c().await;

    println!("✨ Ctrl-C received. Stopping..");
    tx.send_with_urgency(server::Message::CtrlC)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli: Opt = argh::from_env();

    // default command to 'client' if non is passed
    let cmd = cli
        .cmd
        .unwrap_or_else(|| SubOpt::Client(client::CliOpts::default()));

    match cmd {
        SubOpt::Client(opt) => {
            let mut app = App::default();
            let localhost = opt.port.map(|port| format!("127.0.0.1:{}", port));

            if let Some(addr) = opt.host.or(localhost) {
                app.set_host_input(addr.clone());

                if let Ok(addr) = addr.parse::<SocketAddr>() {
                    app.connect_to_server(addr);
                }
            }

            if let Some(name) = opt.username {
                app.set_name_input(name)
            }

            app.start().await?;
        }

        SubOpt::Server(opts) => {
            let log_level_filter = if cli.log_debug {
                log::LevelFilter::Debug
            } else {
                log::LevelFilter::Info
            };

            pretty_env_logger::formatted_builder()
                .filter(Some("termibbl"), log_level_filter)
                .init();

            let port = opts.port;

            // display public ip
            if opts.display_public_ip {
                tokio::spawn(async move {
                    if let Ok(res) = reqwest::get("http://ifconfig.me").await {
                        if let Ok(ip) = res.text().await {
                            println!("Your public IP is {}:{}", ip, port);
                            println!("You can find out your private IP by running \"ip addr\" in the terminal");
                        }
                    }
                });
            }

            let mut default_game_opts: GameOpts = opts.into();
            let default_words = default_game_opts.custom_words.drain(..).collect();
            let server = GameServer::new(default_game_opts, default_words);
            let addr = format!("127.0.0.1:{}", port);

            // listen for ctrl_c
            let ctrlc_abort_handle =
                dispatch_abortable_task(process_ctrl_c(server.sender().clone()));

            println!("🚀 Running Termibbl server on port {}...", port);
            server.listen_on(&addr).await?;
            ctrlc_abort_handle.abort();
        }
    };

    Ok(())
}
