use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, fmt::Display};
use tui::style::Color;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, Serialize, Deserialize)]
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

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct Line {
    pub start: Coord,
    pub end: Coord,
    pub color: CanvasColor,
}

impl Line {
    pub fn new(start: Coord, end: Coord, color: CanvasColor) -> Self {
        Line { start, end, color }
    }
    pub fn coords_in(&self) -> Vec<Coord> {
        line_drawing::Bresenham::new(self.start.into(), self.end.into())
            .map(Coord::from)
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    user: String,
    text: String,
}

impl Message {
    pub fn new(user: String, text: String) -> Self {
        Message { user, text }
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.user, self.text)
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum CanvasColor {
    White,
    Gray,
    DarkGray,
    Black,
    Red,
    LightRed,
    Green,
    LightGreen,
    Blue,
    LightBlue,
    Yellow,
    LightYellow,
    Cyan,
    LightCyan,
    Magenta,
    LightMagenta,
}

impl From<CanvasColor> for Color {
    fn from(c: CanvasColor) -> Self {
        match c {
            CanvasColor::White => Color::White,
            CanvasColor::Gray => Color::Gray,
            CanvasColor::DarkGray => Color::DarkGray,
            CanvasColor::Black => Color::Black,
            CanvasColor::Red => Color::Red,
            CanvasColor::LightRed => Color::LightRed,
            CanvasColor::Green => Color::Green,
            CanvasColor::LightGreen => Color::LightGreen,
            CanvasColor::Blue => Color::Blue,
            CanvasColor::LightBlue => Color::LightBlue,
            CanvasColor::Yellow => Color::Yellow,
            CanvasColor::LightYellow => Color::LightYellow,
            CanvasColor::Cyan => Color::Cyan,
            CanvasColor::LightCyan => Color::LightCyan,
            CanvasColor::Magenta => Color::Magenta,
            CanvasColor::LightMagenta => Color::LightMagenta,
        }
    }
}
