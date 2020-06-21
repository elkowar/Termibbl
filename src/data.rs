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
pub struct AppLine {
    pub start: Coord,
    pub end: Coord,
    pub color: CanvasColor,
}

impl AppLine {
    pub fn new(start: Coord, end: Coord, color: CanvasColor) -> Self {
        AppLine { start, end, color }
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

#[derive(Debug, Copy, Clone)]
pub struct CanvasColor(pub Color);
impl Serialize for CanvasColor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_unit()
    }
}

impl<'de> Deserialize<'de> for CanvasColor {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(CanvasColor(Color::White))
    }
}
