use std::collections::{HashMap, VecDeque};

use ratatui::{
    buffer::Buffer,
    crossterm::event::KeyCode,
    layout::{Constraint, Layout, Rect},
    style::Stylize,
    text::Line,
    widgets::{Block, Clear, Paragraph, Widget},
};

use crate::{
    center,
    db::{Update, User},
};

use super::{Component, State};

#[derive(PartialEq, Eq)]
enum Focus {
    Rooms,
    Messages,
    Chat,
}

struct ChatLine {
    user: User,
    msg: String,
}

pub struct MainView {
    focus: Focus,
    selected_room: usize,
    input_buffer: String,
    room_name: String,
    show_create_room: bool,
    user_cache: HashMap<[u8; 33], User>,
    lines: VecDeque<ChatLine>,
}

impl MainView {
    pub fn new() -> Self {
        Self {
            focus: Focus::Rooms,
            selected_room: 0,
            input_buffer: String::new(),
            room_name: String::new(),
            show_create_room: false,
            user_cache: HashMap::new(),
            lines: VecDeque::new(),
        }
    }

    fn is_focused<'a>(&self, blk: Block<'a>, f: Focus) -> Block<'a> {
        if self.focus == f {
            blk.light_blue()
        } else {
            blk
        }
    }

    fn show_rooms(&self, state: &State, area: Rect, buf: &mut Buffer) {
        let blk = self.is_focused(Block::bordered().title(" Rooms "), Focus::Rooms);

        let mut lines: Vec<Line> = Vec::new();
        for (idx, room) in state.rooms.iter().enumerate() {
            let line: Line = room.name.as_str().into();
            lines.push(if idx == self.selected_room {
                line.white().on_light_blue()
            } else {
                line
            });
        }
        Paragraph::new(lines).block(blk).render(area, buf);
    }

    fn show_messages(&self, area: Rect, buf: &mut Buffer) {
        let blk = self.is_focused(Block::bordered().title(" Messages "), Focus::Messages);
        let mut lines: Vec<Line> = Vec::new();
        for l in self.lines.iter() {
            lines.push(Line::from(vec![
                l.user.name.clone().into(),
                "> ".into(),
                l.msg.clone().into(),
            ]));
        }

        Paragraph::new("").block(blk).render(area, buf);
    }

    fn show_chat(&self, area: Rect, buf: &mut Buffer) {
        let layout = Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).split(area);

        let blk = self.is_focused(Block::bordered().title(" Chat "), Focus::Chat);
        let lines: Vec<Line> = self
            .lines
            .iter()
            .map(|l| Line::from(vec![l.user.name.clone().into(), "> ".into(), l.msg.clone().into()]))
            .collect();
        Paragraph::new(lines).block(blk).render(layout[0], buf);

        let blk = self.is_focused(Block::bordered(), Focus::Chat);
        Paragraph::new(self.input_buffer.as_str())
            .block(blk)
            .render(layout[1], buf);
    }

    fn show_create_room_popup(&self, area: Rect, buf: &mut Buffer) {
        let area = center(area, Constraint::Percentage(15), Constraint::Length(3));

        Clear.render(area, buf);
        let blk = Block::bordered().title(" New Room ");
        Paragraph::new(self.room_name.as_str()).block(blk).render(area, buf);
    }

    fn general_keys(&mut self, state: &mut State, ev: KeyCode) {
        match ev {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.quit = true;
            }
            KeyCode::Char('1') => {
                self.focus = Focus::Rooms;
                state.ignore_space = false;
            }
            KeyCode::Char('2') => {
                self.focus = Focus::Messages;
                state.ignore_space = false;
            }
            KeyCode::Char('3') => {
                self.focus = Focus::Chat;
                state.ignore_space = true;
            }
            KeyCode::Char('c') => {
                self.show_create_room = true;
                state.ignore_space = true;
            }
            _ => {}
        }
    }
}

impl Component for MainView {
    fn on_update(&mut self, state: &mut State, update: &Update) {
        match update {
            Update::RoomMessage { user, room, message } => {
                if self.room_name == *room {
                    let suser = self
                        .user_cache
                        .get(user)
                        .cloned()
                        .unwrap_or_else(|| state.db.user_by_key(*user).unwrap());
                    self.lines.push_back(ChatLine {
                        user: suser,
                        msg: message.clone(),
                    });
                }
            }
        }
    }

    fn on_press(&mut self, state: &mut State, ev: KeyCode) {
        if self.show_create_room {
            match ev {
                KeyCode::Esc => {
                    self.show_create_room = false;
                    self.room_name.clear();
                    state.ignore_space = false;
                }
                KeyCode::Enter => {
                    self.show_create_room = false;

                    if let Some(user) = &state.user {
                        match state.db.create_room(self.room_name.clone(), user.user.pubkey) {
                            Ok(_) => {
                                state.update_rooms();
                            }
                            Err(e) => {
                                tracing::error!("{e}");
                            }
                        }
                    }

                    self.room_name.clear();
                    state.ignore_space = false;
                }
                KeyCode::Char(a) => {
                    self.room_name.push(a);
                }
                _ => {}
            }
        } else {
            match self.focus {
                Focus::Rooms => {
                    self.general_keys(state, ev);
                    match ev {
                        KeyCode::Char('j') => {
                            self.room_name = state.rooms[self.selected_room].name.clone();
                            self.lines.clear();
                            self.user_cache = state.db.users_in_room(&self.room_name).unwrap();

                            match state.db.last_20_lines(&self.room_name) {
                                Ok(lines) => {
                                    for line in lines {
                                        self.lines.push_back(ChatLine {
                                            user: self.user_cache.get(&line.0).cloned().unwrap_or_else(|| User {
                                                id: -1,
                                                pubkey: [0u8; 33],
                                                name: "unknown".into(),
                                            }),
                                            msg: line.1,
                                        });
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("{e}");
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Focus::Messages => {
                    self.general_keys(state, ev);
                }
                Focus::Chat => match ev {
                    KeyCode::Backspace => {
                        self.input_buffer.pop();
                    }
                    KeyCode::Esc => {
                        self.input_buffer.clear();
                        self.focus = Focus::Rooms;
                        state.ignore_space = false;
                    }
                    KeyCode::Enter => {
                        if let Some(puser) = &state.user {
                            let room = &state.rooms[self.selected_room];
                            if let Err(e) = state.db.create_message(puser, room, &self.input_buffer) {
                                tracing::error!("{e}");
                            }
                            self.input_buffer.clear();
                        }
                    }
                    KeyCode::Char(a) => {
                        self.input_buffer.push(a);
                    }
                    _ => {}
                },
            }
        }
    }

    fn render(&self, state: &State, area: Rect, buf: &mut Buffer) {
        let layout = Layout::horizontal([Constraint::Percentage(25), Constraint::Percentage(75)]).split(area);
        let left = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(layout[0]);

        self.show_rooms(state, left[0], buf);
        self.show_messages(left[1], buf);
        self.show_chat(layout[1], buf);

        if self.show_create_room {
            self.show_create_room_popup(area, buf);
        }
    }
}
