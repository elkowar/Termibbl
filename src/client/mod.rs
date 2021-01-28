pub use crate::*;
pub mod app;
pub mod error;
pub mod ui;

#[derive(FromArgs)]
/// play Skribbl.io-like games in the Termibbl
#[argh(subcommand, name = "client")]
pub struct CliOpts {
    #[argh(positional)]
    ///username to connect as.
    pub username: String,

    #[argh(option, short = 'a')]
    /// address of server to connect to.
    pub addr: String,
}
