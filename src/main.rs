use std::io::{stdout, Write};

use app::App;
use crossterm::{
    event::{read, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    Result,
};

use tui::{backend::CrosstermBackend, Terminal};

pub mod app;
pub mod ui;

pub const CANVAS_SIZE: (usize, usize) = (100, 50);

fn main() -> Result<()> {
    let mut app = App::new();
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    execute!(stdout(), EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    loop {
        match read()? {
            Event::Mouse(evt) => {
                app.apply_mouse_event(evt);
            }
            Event::Key(evt) => {
                app.apply_key_event(evt);
            }
            _ => {}
        }
        if app.should_stop {
            break;
        }
        ui::draw(&app, &mut terminal);
    }

    execute!(stdout(), DisableMouseCapture)?;
    execute!(stdout(), LeaveAlternateScreen)?;
    disable_raw_mode()
}
