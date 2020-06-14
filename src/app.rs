use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use std::cmp::Ordering;
use tui::style::Color;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord)]
pub struct Coord(pub u16, pub u16);

impl PartialOrd for Coord {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self.0 < other.0 && self.1 < other.1 {
            Some(Ordering::Less)
        } else if self.0 == other.0 && self.1 == other.1 {
            Some(Ordering::Equal)
        } else {
            Some(Ordering::Greater)
        }
    }
}

impl Coord {
    pub fn within(&self, a: &Coord, b: &Coord) -> bool {
        self > a.min(b) && self < a.max(b)
    }
}

impl From<(i16, i16)> for Coord {
    fn from(x: (i16, i16)) -> Self {
        Coord(x.0 as u16, x.1 as u16)
    }
}
impl From<Coord> for (i16, i16) {
    fn from(c: Coord) -> Self {
        (c.0 as i16, c.1 as i16)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct AppLine {
    pub start: Coord,
    pub end: Coord,
    pub color: Color,
}

impl AppLine {
    pub fn new(start: Coord, end: Coord, color: Color) -> Self {
        AppLine { start, end, color }
    }
    pub fn coords_in(&self) -> Vec<Coord> {
        line_drawing::Bresenham::new(self.start.into(), self.end.into())
            .map(Coord::from)
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct AppCanvas {
    pub lines: Vec<AppLine>,
    pub last_mouse_pos: Option<Coord>,
    pub current_color: Color,
}

impl AppCanvas {
    fn new() -> Self {
        AppCanvas {
            lines: Vec::new(),
            last_mouse_pos: None,
            current_color: Color::White,
        }
    }
}

impl AppCanvas {
    pub fn apply_mouse_event(&mut self, evt: MouseEvent) -> Option<()> {
        match evt {
            MouseEvent::Down(_, x, y, _) => {
                self.last_mouse_pos = Some(Coord(x, y));
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

#[derive(Debug, Clone)]
pub struct App {
    pub canvas: AppCanvas,
    pub chat: Vec<String>,
    pub input: String,
    pub should_stop: bool,
}

impl App {
    pub fn new() -> App {
        App {
            canvas: AppCanvas::new(),
            chat: Vec::new(),
            input: String::new(),
            should_stop: false,
        }
    }
    pub fn apply_mouse_event(&mut self, evt: MouseEvent) {
        self.canvas.apply_mouse_event(evt);
    }
    pub fn apply_key_event(&mut self, evt: KeyEvent) {
        let KeyEvent { code, modifiers } = evt;
        match code {
            KeyCode::Esc => self.should_stop = true,
            KeyCode::Char(c) => self.input.push(c),
            KeyCode::Enter => {
                self.chat.push(self.input.clone());
                self.input.clear();
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            _ => {}
        }
    }
}
