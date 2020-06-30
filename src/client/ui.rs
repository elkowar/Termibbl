use crate::{
    client::app::{App, AppCanvas, Chat},
    client::error::Result,
    data::Coord,
    server::skribbl::SkribblState,
};

use super::Username;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::Style,
    widgets::{Block, Borders, List, Paragraph, Text, Widget},
    Terminal,
};

pub fn draw<B: Backend>(app: &mut App, terminal: &mut Terminal<B>) -> Result<()> {
    let dimensions = app.canvas.dimensions;
    terminal.draw(|mut f| {
        use Constraint::*;
        let size = f.size();
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .margin(0)
            .constraints(
                [
                    Length(dimensions.0 as u16),
                    Length(if size.width < dimensions.0 as u16 {
                        size.width
                    } else {
                        size.width - dimensions.0 as u16
                    }),
                ]
                .as_ref(),
            )
            .split(size);

        let left_chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(0)
            .constraints([Length(dimensions.1 as u16), Percentage(100)].as_ref())
            .split(main_chunks[0]);

        let canvas_area = {
            let mut x = left_chunks[0];
            x.height = x.height.min(dimensions.1 as u16);
            x
        };

        let canvas_widget = CanvasWidget::new(
            &app.canvas,
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().bg(app.current_color.into())),
        );

        if let Some(skribbl_state) = app.game_state.as_mut() {
            let skribbl_widget = SkribblStateWidget::new(
                &skribbl_state,
                &app.session.username,
                Block::default().borders(Borders::ALL),
            );
            f.render_widget(skribbl_widget, left_chunks[1]);
        }

        f.render_widget(canvas_widget, canvas_area);

        let chat_widget = ChatWidget::new(&app.chat, Block::default().borders(Borders::ALL));
        f.render_widget(chat_widget, main_chunks[1]);
    })?;
    Ok(())
}

pub struct CanvasWidget<'a, 't> {
    block: Block<'a>,
    canvas: &'t AppCanvas,
}

impl<'a, 't> CanvasWidget<'a, 't> {
    pub fn new(canvas: &'t AppCanvas, block: Block<'a>) -> CanvasWidget<'a, 't> {
        CanvasWidget { block, canvas }
    }
}

impl<'a, 't, 'b> Widget for CanvasWidget<'a, 't> {
    fn render(self, area: tui::layout::Rect, buf: &mut tui::buffer::Buffer) {
        self.block.render(area, buf);
        let area = self.block.inner(area);

        for line in self.canvas.lines.iter() {
            for cell in line.coords_in() {
                if cell.within(
                    &Coord(area.x, area.y),
                    &Coord(area.x + area.width, area.y + area.height),
                ) {
                    buf.get_mut(cell.0, cell.1).set_bg(line.color.into());
                }
            }
        }
        let swatch_size = area.width / self.canvas.palette.len() as u16;
        for (idx, col) in self.canvas.palette.iter().enumerate() {
            for offset in 0..swatch_size {
                buf.get_mut(offset + (idx as u16 * swatch_size), 0)
                    .set_bg((*col).into());
            }
        }
    }
}

pub struct ChatWidget<'a, 't> {
    block: Block<'a>,
    chat: &'t Chat,
}

impl<'a, 't> ChatWidget<'a, 't> {
    pub fn new(chat: &'t Chat, block: Block<'a>) -> ChatWidget<'a, 't> {
        ChatWidget { block, chat }
    }
}
impl<'a, 't, 'b> Widget for ChatWidget<'a, 't> {
    fn render(self, area: tui::layout::Rect, buf: &mut tui::buffer::Buffer) {
        use Constraint::*;
        self.block.render(area, buf);
        let area = self.block.inner(area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(0)
            .constraints([Length(3), Percentage(100)].as_ref())
            .split(area);

        Paragraph::new([Text::Raw(self.chat.input.clone().into())].iter())
            .block(Block::default().borders(Borders::ALL).title("Your message"))
            .render(chunks[0], buf);

        List::new(
            self.chat
                .messages
                .iter()
                .rev()
                .map(|msg| Text::raw(format!("{}", msg))),
        )
        .block(Block::default().borders(Borders::ALL).title("Chat"))
        .render(chunks[1], buf);
    }
}

pub struct SkribblStateWidget<'a, 't> {
    block: Block<'a>,
    state: &'t SkribblState,
    username: &'t Username,
}
impl<'a, 't> SkribblStateWidget<'a, 't> {
    pub fn new(
        state: &'t SkribblState,
        username: &'t Username,
        block: Block<'a>,
    ) -> SkribblStateWidget<'a, 't> {
        SkribblStateWidget {
            block,
            state,
            username,
        }
    }
}

impl<'a, 't, 'b> Widget for SkribblStateWidget<'a, 't> {
    fn render(self, area: tui::layout::Rect, buf: &mut tui::buffer::Buffer) {
        self.block.render(area, buf);
        let area = self.block.inner(area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(0)
            .constraints([Constraint::Length(1), Constraint::Percentage(100)].as_ref())
            .split(area);

        let is_drawing = self.state.drawing_user == *self.username;

        let current_word_representation = if is_drawing {
            self.state.current_word.to_string()
        } else {
            self.state
                .current_word
                .replace(|c: char| !c.is_whitespace(), &"?")
        };

        Paragraph::new(
            [Text::Raw(
                format!(
                    "{} {}",
                    self.state.drawing_user,
                    format!("drawing {}", current_word_representation)
                )
                .into(),
            )]
            .iter(),
        )
        .render(chunks[0], buf);

        List::new(
            self.state
                .player_states
                .iter()
                .map(|(username, player_state)| {
                    Text::styled(
                        format!(
                            "{}: {} {}",
                            username,
                            player_state.score,
                            if self.state.player_states.get(username).map(|x| x.has_solved)
                                == Some(true)
                            {
                                "Solved!"
                            } else {
                                ""
                            }
                        ),
                        if self.state.drawing_user == *username {
                            Style::default().bg(tui::style::Color::Red)
                        } else {
                            Style::default()
                        },
                    )
                }),
        )
        .block(Block::default().borders(Borders::ALL).title("Players"))
        .render(chunks[1], buf);
    }
}
