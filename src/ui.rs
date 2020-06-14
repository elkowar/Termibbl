use crate::{
    app::{App, AppCanvas, Coord},
    CANVAS_SIZE,
};

use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, Widget},
    Terminal,
};

pub fn draw<B: Backend>(app: &App, terminal: &mut Terminal<B>) {
    terminal
        .draw(|mut f| {
            use Constraint::*;
            let size = f.size();
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .margin(0)
                .constraints([Length(CANVAS_SIZE.0 as u16)].as_ref())
                .split(size);

            let draw_canvas =
                DrawCanvas::new(&app.canvas).block(Block::default().borders(Borders::ALL));
            f.render_widget(draw_canvas, main_chunks[0]);
        })
        .unwrap();
}

pub struct DrawCanvas<'a, 't> {
    block: Option<Block<'a>>,
    canvas: &'t AppCanvas,
}

impl<'a, 't> DrawCanvas<'a, 't> {
    pub fn new(c: &AppCanvas) -> DrawCanvas {
        DrawCanvas {
            block: None,
            canvas: c,
        }
    }
    pub fn block(mut self, block: Block<'a>) -> DrawCanvas<'a, 't> {
        self.block = Some(block);
        self
    }
}

impl<'a, 't, 'b> Widget for DrawCanvas<'a, 't> {
    fn render(mut self, area: tui::layout::Rect, buf: &mut tui::buffer::Buffer) {
        let area = match self.block {
            Some(ref mut b) => {
                b.render(area, buf);
                b.inner(area)
            }
            None => area,
        };

        for line in &self.canvas.lines {
            for cell in line.coords_in() {
                if cell.within(
                    &Coord(area.x, area.y),
                    &Coord(area.x + area.width, area.y + area.height),
                ) {
                    buf.get_mut(cell.0, cell.1).set_bg(line.color);
                }
            }
        }
    }
}
