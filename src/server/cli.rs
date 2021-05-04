use argh::FromArgs;
use std::{fs, io::Read};

use super::skribbl;
use crate::data::{Coord, GameOpts};

pub const DEFAULT_PORT: u16 = 9001;
pub const DEFAULT_DIMENSIONS: Coord = (100, 900);
pub const DEFAULT_WORDS: &str = include_str!("words_dump");
pub const ROOM_KEY_LENGTH: usize = 5;

type ParseResult<T> = std::result::Result<T, String>;

fn parse_dimension(s: &str) -> ParseResult<Coord> {
    let mut split = s
        .split('x')
        .map(str::parse)
        .filter_map(std::result::Result::ok);

    split
        .next()
        .and_then(|width| split.next().map(|height| (width, height)))
        .ok_or_else(|| "could not parse dimensions".to_owned())
}

fn parse_words_file(path: &str) -> ParseResult<String> {
    let mut words = String::new();

    // read from file
    fs::File::open(path)
        .and_then(|mut f| f.read_to_string(&mut words))
        .map_err(|e| e.to_string())?;

    Ok(words)
}

/// host a Termibbl session
#[derive(FromArgs)]
#[argh(subcommand, name = "server")]
pub struct CliOpts {
    /// port for server to run on
    #[argh(option, short = 'p', default = "DEFAULT_PORT")]
    pub port: u16,

    /// whether to show public ip when server starts
    #[argh(switch, short = 'y')]
    pub display_public_ip: bool,

    #[argh(option, default = "skribbl::DEFAULT_DRAW_TIME")]
    /// default drawing duration in seconds
    draw_time: u64,

    #[argh(option, default = "skribbl::DEFAULT_NUM_OF_ROUNDS")]
    /// default number of rounds per game
    rounds: usize,

    /// default canvas dimensions <width>x<height>
    #[argh(option, default = "DEFAULT_DIMENSIONS", from_str_fn(parse_dimension))]
    dimensions: Coord,

    /// optional path to custom word list
    #[argh(option, short = 'w', from_str_fn(parse_words_file))]
    words: Option<String>,
}

impl From<CliOpts> for GameOpts {
    fn from(mut opt: CliOpts) -> Self {
        GameOpts {
            dimensions: opt.dimensions,
            number_of_rounds: opt.rounds,
            draw_time: opt.draw_time as usize,
            custom_words: opt
                .words
                .take()
                .unwrap_or_else(|| DEFAULT_WORDS.to_string())
                .lines()
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>(),
            only_custom_words: false,
        }
    }
}
