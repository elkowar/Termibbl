use crate::{
    client::app::{App, AppCanvas, Chat},
    data::Coord,
    CANVAS_SIZE,
};

use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::Style,
    widgets::{Block, Borders, List, Paragraph, Text, Widget},
    Terminal,
};

pub fn draw<B: Backend>(app: &mut App, terminal: &mut Terminal<B>) {
    terminal
        .draw(|mut f| {
            use Constraint::*;
            let size = f.size();
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .margin(0)
                .constraints(
                    [
                        Length(CANVAS_SIZE.0 as u16),
                        Length(if size.width < CANVAS_SIZE.0 as u16 {
                            size.width
                        } else {
                            size.width - CANVAS_SIZE.0 as u16
                        }),
                    ]
                    .as_ref(),
                )
                .split(size);

            let canvas_area = {
                let mut x = main_chunks[0];
                x.height = x.height.min(CANVAS_SIZE.1 as u16);
                x
            };

            let canvas_widget =
                CanvasWidget::new(&app.canvas, Block::default().borders(Borders::ALL));
            f.render_widget(canvas_widget, canvas_area);

            let chat_widget = ChatWidget::new(&app.chat, Block::default().borders(Borders::ALL));
            f.render_widget(chat_widget, main_chunks[1]);
        })
        .unwrap();
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
        self.block
            .border_style(Style::default().bg(self.canvas.current_color.0))
            .render(area, buf);
        let area = self.block.inner(area);

        for line in self.canvas.lines.iter() {
            for cell in line.coords_in() {
                if cell.within(
                    &Coord(area.x, area.y),
                    &Coord(area.x + area.width, area.y + area.height),
                ) {
                    buf.get_mut(cell.0, cell.1).set_bg(line.color.0);
                }
            }
        }
        let swatch_size = area.width / self.canvas.palette.len() as u16;
        for (idx, col) in self.canvas.palette.iter().enumerate() {
            for offset in 0..swatch_size {
                buf.get_mut(offset + (idx as u16 * swatch_size), 0)
                    .set_bg(col.0.clone());
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
                .map(|msg| Text::raw(format!("{}", msg))),
        )
        .block(Block::default().borders(Borders::ALL).title("Chat"))
        .render(chunks[1], buf);
    }
}
