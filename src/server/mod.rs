use std::io::Read;

use argh::FromArgs;
use log::{debug, info};

pub mod server;
pub mod skribbl;

const DIMEN: (usize, usize) = (900, 60);
const ROUND_DURATION: usize = 120;
const ROUNDS: usize = 3;

#[derive(FromArgs)]
/// host a Termibbl session
#[argh(subcommand, name = "server")]
pub struct CliOpts {
    /// port for server to run on
    #[argh(option, short = 'p')]
    pub port: u32,

    /// whether to show public ip when server starts
    #[argh(switch, short = 'y')]
    pub display_public_ip: bool,

    #[argh(option, default = "ROUND_DURATION")]
    /// default round duration in seconds
    round_duration: usize,

    #[argh(option, default = "ROUNDS")]
    /// default number of rounds per game
    rounds: usize,

    /// default canvas dimensions <width>x<height>
    #[argh(option, from_str_fn(parse_dimension), default = "DIMEN")]
    dimensions: (usize, usize),

    /// optional path to custom word list
    #[argh(option, from_str_fn(read_words_file))]
    words: Option<Vec<String>>,
}

fn parse_dimension(s: &str) -> Result<(usize, usize), String> {
    let mut split = s
        .split('x')
        .map(str::parse)
        .filter_map(std::result::Result::ok);

    split
        .next()
        .and_then(|width| split.next().map(|height| (width, height)))
        .ok_or_else(|| "could not parse dimensions".to_owned())
}

fn read_words_file(path: &str) -> Result<Vec<String>, String> {
    info!("reading words from file {}", path);

    let mut words = String::new();
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;

    file.read_to_string(&mut words).map_err(|e| e.to_string())?;

    debug!("read {} words from file", words.len());

    Ok(words
        .lines()
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect::<Vec<String>>())
}

#[derive(Clone)]
pub struct GameOpts {
    pub dimensions: (usize, usize),
    pub words: Vec<String>,
    pub number_of_rounds: usize,
    pub round_duration: usize,
}

impl From<CliOpts> for GameOpts {
    fn from(opt: CliOpts) -> Self {
        let default_words = opt.words.unwrap_or_else(Vec::new);
        let default_dimensions = opt.dimensions;
        let default_round_duration = opt.round_duration;
        let default_number_of_rounds = opt.rounds;

        Self {
            dimensions: default_dimensions,
            words: default_words,
            number_of_rounds: default_number_of_rounds,
            round_duration: default_round_duration,
        }
    }
}
