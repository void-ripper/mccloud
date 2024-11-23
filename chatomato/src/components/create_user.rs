use ratatui::{
    buffer::Buffer,
    crossterm::event::KeyCode,
    layout::{Constraint, Layout, Rect},
    widgets::{Block, Clear, Paragraph, Widget},
};

use crate::center;

use super::Component;

pub struct CreateUser {
    inbuffer: String,
    show: bool,
}

impl CreateUser {
    pub fn new(show: bool) -> Self {
        Self {
            inbuffer: String::new(),
            show,
        }
    }
}

impl Component for CreateUser {
    fn on_press(&mut self, ev: KeyCode) {
        match ev {
            KeyCode::Enter => {
                self.inbuffer.clear();
                self.show = false;
            }
            _ => {}
        }
    }
}

impl Widget for &CreateUser {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        if self.show {
            let center = center(area, Constraint::Percentage(50), Constraint::Percentage(50));

            Clear.render(center, buf);
            Block::bordered().render(center, buf);
            let inner = Rect::new(center.x + 1, center.y + 1, center.width - 2, center.height - 2);

            let layout = Layout::vertical([Constraint::Length(3)]).split(inner);

            let blk = Block::new().title("name");
            Paragraph::new(self.inbuffer.as_str()).block(blk).render(layout[0], buf);
        }
    }
}
