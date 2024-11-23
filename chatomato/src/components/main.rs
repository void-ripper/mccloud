use ratatui::{
    buffer::Buffer,
    crossterm::event::KeyCode,
    layout::{Constraint, Layout, Rect},
    style::Stylize,
    widgets::{Block, Paragraph, Widget},
};

use super::{Component, State};

#[derive(PartialEq, Eq)]
enum Focus {
    Rooms,
    Messages,
    Chat,
}

pub struct MainView {
    focus: Focus,
    input_buffer: String,
}

impl MainView {
    pub fn new() -> Self {
        Self {
            focus: Focus::Rooms,
            input_buffer: String::new(),
        }
    }

    fn is_focused<'a>(&self, blk: Block<'a>, f: Focus) -> Block<'a> {
        if self.focus == f {
            blk.light_blue()
        } else {
            blk
        }
    }
    fn show_rooms(&self, area: Rect, buf: &mut Buffer) {
        let blk = self.is_focused(Block::bordered().title(" Rooms "), Focus::Rooms);

        Paragraph::new("").block(blk).render(area, buf);
    }

    fn show_messages(&self, area: Rect, buf: &mut Buffer) {
        let blk = self.is_focused(Block::bordered().title(" Messages "), Focus::Messages);
        Paragraph::new("").block(blk).render(area, buf);
    }

    fn show_chat(&self, area: Rect, buf: &mut Buffer) {
        let layout = Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).split(area);

        let blk = self.is_focused(Block::bordered().title(" Chat "), Focus::Chat);
        Paragraph::new("").block(blk).render(layout[0], buf);

        let blk = self.is_focused(Block::bordered(), Focus::Chat);
        Paragraph::new(self.input_buffer.as_str())
            .block(blk)
            .render(layout[1], buf);
    }
}

impl Component for MainView {
    fn on_press(&mut self, state: &mut State, ev: KeyCode) {
        match ev {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.quit = true;
            },
            KeyCode::Char('1') => {
                self.focus = Focus::Rooms;
            }
            KeyCode::Char('2') => {
                self.focus = Focus::Messages;
            }
            KeyCode::Char('3') => {
                self.focus = Focus::Chat;
            }
            _ => {}
        }
    }
}

impl Widget for &MainView {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let layout = Layout::horizontal([Constraint::Percentage(25), Constraint::Percentage(75)]).split(area);
        let left = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(layout[0]);

        self.show_rooms(left[0], buf);
        self.show_messages(left[1], buf);
        self.show_chat(layout[1], buf);
    }
}
