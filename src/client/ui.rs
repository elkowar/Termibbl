use crate::{
    client::app::{App, AppCanvas},
    client::error::Result,
    data::{Coord, Message},
    server::skribbl::{PlayerState, SkribblState},
};

use super::Username;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
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

        let canvas_widget = CanvasWidget::new(
            &app.canvas,
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(app.current_color.into())),
        );

        let game_state_height = app
            .game_state
            .as_ref()
            .map(|x| x.player_states.len() + 3)
            .unwrap_or(0) as u16;

        let sidebar_chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(0)
            .constraints([Length(game_state_height), Percentage(100)].as_ref())
            .split(main_chunks[1]);

        if let Some(skribbl_state) = app.game_state.as_mut() {
            let skribbl_widget = SkribblStateWidget::new(
                &skribbl_state,
                &app.session.username,
                app.remaining_time.unwrap_or(0),
                Block::default().borders(Borders::NONE),
            );
            f.render_widget(skribbl_widget, sidebar_chunks[0]);
        }

        let canvas_rect = Rect {
            height: u16::min(dimensions.1 as u16, main_chunks[0].height),
            ..main_chunks[0]
        };
        f.render_widget(canvas_widget, canvas_rect);

        let displayed_messages = (&app.chat.messages)
            .iter()
            .filter(|msg| match msg {
                Message::SystemMsg(_) => true,
                Message::UserMsg(username, _) => app.game_state.as_ref().map_or(true, |state| {
                    app.is_drawing()
                        || app.own_player().map_or(false, |x| x.has_solved)
                        || (&state.drawing_user != username
                            && !state
                                .player_states
                                .get(&username)
                                .map_or(false, |player_state| player_state.has_solved))
                }),
            })
            .collect::<Vec<_>>();

        let chat_widget = ChatWidget::new(
            displayed_messages.as_slice(),
            &app.chat.input,
            Block::default().borders(Borders::NONE),
        );
        f.render_widget(chat_widget, sidebar_chunks[1]);
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
    messages: &'t [&'t Message],
    input: &'t str,
}

impl<'a, 't> ChatWidget<'a, 't> {
    pub fn new(messages: &'t [&Message], input: &'t str, block: Block<'a>) -> ChatWidget<'a, 't> {
        ChatWidget {
            block,
            messages,
            input,
        }
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

        Paragraph::new([Text::Raw(self.input.clone().into())].iter())
            .block(Block::default().borders(Borders::ALL).title("Your message"))
            .render(chunks[0], buf);

        List::new(self.messages.iter().rev().map(|msg| {
            Text::styled(
                format!("{}", msg),
                if msg.is_system() {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default()
                },
            )
        }))
        .block(Block::default().borders(Borders::ALL).title("Chat"))
        .render(chunks[1], buf);
    }
}

pub struct SkribblStateWidget<'a, 't> {
    block: Block<'a>,
    state: &'t SkribblState,
    username: &'t Username,
    remaining_time: u32,
}
impl<'a, 't> SkribblStateWidget<'a, 't> {
    pub fn new(
        state: &'t SkribblState,
        username: &'t Username,
        remaining_time: u32,
        block: Block<'a>,
    ) -> SkribblStateWidget<'a, 't> {
        SkribblStateWidget {
            block,
            state,
            username,
            remaining_time,
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
            self.state.current_word().to_string()
        } else {
            self.state.hinted_current_word().to_string()
        };

        Paragraph::new(
            [Text::Styled(
                format!(
                    "{} drawing {}",
                    self.state.drawing_user, current_word_representation
                )
                .into(),
                if is_drawing {
                    Style::default().bg(Color::Red)
                } else {
                    Style::default()
                },
            )]
            .iter(),
        )
        .render(chunks[0], buf);

        let mut sorted_player_entries = self
            .state
            .player_states
            .iter()
            .collect::<Vec<(&Username, &PlayerState)>>();
        sorted_player_entries.sort_by_key(|(x, _)| *x);

        List::new(
            sorted_player_entries
                .into_iter()
                .map(|(username, player_state)| {
                    Text::styled(
                        format!("{}: {}", username, player_state.score,),
                        if self.state.drawing_user == *username {
                            Style::default().bg(tui::style::Color::Cyan)
                        } else if self.state.has_solved(username) {
                            Style::default().fg(tui::style::Color::Green)
                        } else {
                            Style::default()
                        },
                    )
                }),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(&format!("Players [time: {}]", self.remaining_time)),
        )
        .render(chunks[1], buf);
    }
}
