mod app;
mod app_server;
mod error;
mod ui;

pub use app::App;
pub use crossterm::event::Event as InputEvent;

use argh::FromArgs;

/// play Skribbl.io-like games in the Termibbl
#[derive(FromArgs, Default)]
#[argh(subcommand, name = "client")]
pub struct CliOpts {
    #[argh(positional)]
    ///username to connect as.
    pub username: Option<String>,

    #[argh(option, short = 'h')]
    /// address of server to connect to.
    pub host: Option<String>,

    #[argh(option, short = 'p')]
    /// port of the local server to connect
    pub port: Option<usize>,
}
