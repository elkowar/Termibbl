use crate::{
    data::{AppLine, CanvasColor, Coord, Message},
    server::ToClientMsg,
    ui, ClientEvent, CANVAS_SIZE, PALETTE_SIZE,
};
use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use std::{cmp::Ordering, sync::mpsc};
use tui::{backend::Backend, style::Color, Terminal};

#[derive(Debug, Clone)]
pub struct AppCanvas {
    pub palette: Vec<CanvasColor>,
    pub lines: Vec<AppLine>,
    pub last_mouse_pos: Option<Coord>,
    pub current_color: CanvasColor,
}

impl AppCanvas {
    fn new() -> Self {
        AppCanvas {
            lines: Vec::new(),
            last_mouse_pos: None,
            current_color: CanvasColor(Color::White),
            palette: [
                CanvasColor(Color::White),
                CanvasColor(Color::Gray),
                CanvasColor(Color::DarkGray),
                CanvasColor(Color::Black),
                CanvasColor(Color::Red),
                CanvasColor(Color::LightRed),
                CanvasColor(Color::Green),
                CanvasColor(Color::LightGreen),
                CanvasColor(Color::Blue),
                CanvasColor(Color::LightBlue),
                CanvasColor(Color::Yellow),
                CanvasColor(Color::LightYellow),
                CanvasColor(Color::Cyan),
                CanvasColor(Color::LightCyan),
                CanvasColor(Color::Magenta),
                CanvasColor(Color::LightMagenta),
            ]
            .to_vec(),
        }
    }
}

impl AppCanvas {
    pub fn apply_mouse_event(&mut self, evt: MouseEvent) -> Option<()> {
        match evt {
            MouseEvent::Down(_, x, y, _) => {
                if y == 0 {
                    let swatch_size = CANVAS_SIZE.0 / self.palette.len() as usize;
                    let selected_color = self.palette.get(x as usize / swatch_size);
                    match selected_color {
                        Some(color) => self.current_color = color.clone(),
                        _ => {}
                    }
                } else {
                    self.last_mouse_pos = Some(Coord(x, y));
                }
            }
            MouseEvent::Up(_, x, y, _) => {
                self.last_mouse_pos = None;
            }
            MouseEvent::Drag(_, x, y, _) => {
                let mouse_pos = Coord(x, y);
                self.lines.push(AppLine::new(
                    self.last_mouse_pos.unwrap_or(mouse_pos),
                    mouse_pos,
                    self.current_color,
                ));
                self.last_mouse_pos = Some(mouse_pos);
            }
            _ => {}
        }
        Some(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct Chat {
    pub input: String,
    pub messages: Vec<Message>,
}

impl Chat {
    fn apply_key_event(&mut self, code: &KeyCode) {
        match code {
            KeyCode::Char(c) => self.input.push(*c),
            KeyCode::Enter => {
                self.input = String::new();
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
pub struct App {
    pub canvas: AppCanvas,
    pub chat: Chat,
}

impl App {
    pub fn new() -> App {
        App {
            canvas: AppCanvas::new(),
            chat: Chat::default(),
        }
    }

    pub fn handle_event(&mut self, evt: ClientEvent) {
        match evt {
            ClientEvent::KeyInput(KeyEvent { code, modifiers }) => match code {
                code => self.chat.apply_key_event(&code),
            },
            ClientEvent::MouseInput(mouse_evt) => {
                self.canvas.apply_mouse_event(mouse_evt);
            }
            ClientEvent::ServerMessage(m) => match m {
                ToClientMsg::NewMessage(message) => self.chat.messages.push(message),
            },
        }
    }

    pub fn run<B: Backend>(
        &mut self,
        mut terminal: &mut Terminal<B>,
        chan: mpsc::Receiver<ClientEvent>,
    ) {
        loop {
            ui::draw(self, &mut terminal);
            let event = chan.recv();
            match event {
                Ok(event) => {
                    self.handle_event(event);
                }
                Err(err) => {
                    eprintln!("{}", err);
                }
            }
        }
    }
}
