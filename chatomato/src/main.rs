use std::sync::Arc;

use components::{create_user::CreateUser, main::MainView, popup::Popup, Component};
use db::{Database, PrivateUser};
use indexmap::IndexSet;
use k256::{elliptic_curve::sec1::ToEncodedPoint, SecretKey};
use ratatui::{
    buffer::Buffer,
    crossterm::event::{self, KeyCode, KeyEventKind},
    layout::{Constraint, Flex, Layout, Rect},
    widgets::Widget,
};

mod components;
mod config;
mod db;
mod error;

pub fn center(area: Rect, horizontal: Constraint, vertical: Constraint) -> Rect {
    let [area] = Layout::horizontal([horizontal]).flex(Flex::Center).areas(area);
    let [area] = Layout::vertical([vertical]).flex(Flex::Center).areas(area);
    area
}

pub fn bottom_right(area: Rect, horizontal: Constraint, vertical: Constraint) -> Rect {
    let [area] = Layout::horizontal([horizontal]).margin(1).flex(Flex::End).areas(area);
    let [area] = Layout::vertical([vertical]).margin(1).flex(Flex::End).areas(area);
    area
}

pub struct State {
    db: Arc<Database>,
    rooms: IndexSet<String>,
    quit: bool,
    user: Option<PrivateUser>,
    next: Option<Active>,
}

impl State {
    pub(crate) fn update_rooms(&mut self) {
        let rooms = self.db.list_rooms().unwrap();
        self.rooms.extend(rooms.iter().map(|r| r.name.clone()));
    }
}

enum Active {
    Main(MainView),
    CreateUser(CreateUser),
}

impl Active {
    fn on_press(&mut self, state: &mut State, ev: KeyCode) {
        match self {
            Self::Main(main) => main.on_press(state, ev),
            Self::CreateUser(cu) => cu.on_press(state, ev),
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        match self {
            Self::Main(main) => main.render(area, buf),
            Self::CreateUser(cu) => cu.render(area, buf),
        }
    }
}

struct App {
    state: State,
    active: Active,
    popup: Popup,
}

impl App {
    fn on_press(&mut self, ev: KeyCode) {
        self.active.on_press(&mut self.state, ev);
        self.popup.on_press(&mut self.state, ev);
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        self.active.render(area, buf);
        self.popup.render(area, buf);
    }
}

fn main() {
    let cfg = config::load().unwrap();

    if !cfg.data.exists() {
        std::fs::create_dir_all(&cfg.data).unwrap();
    }

    let prikey = cfg.data.join("private.key");
    let exists = prikey.exists();
    let db = Arc::new(Database::new(cfg.clone()).unwrap());
    let user = if exists {
        let data = std::fs::read(&prikey).unwrap();
        let secret = SecretKey::from_slice(&data).unwrap();
        let mut pubkey = [0u8; 33];
        pubkey.copy_from_slice(&secret.public_key().to_encoded_point(true).to_bytes());
        let user = db.user_by_key(pubkey).unwrap();
        Some(PrivateUser { user, secret })
    } else {
        None
    };
    let mut app = App {
        state: State {
            rooms: IndexSet::new(),
            db: db.clone(),
            quit: false,
            user,
            next: None,
        },
        active: if exists {
            Active::Main(MainView::new())
        } else {
            Active::CreateUser(CreateUser::new(
                cfg.clone(),
                Box::new(move |state, name| match db.create_user(name) {
                    Ok(user) => {
                        state.user = Some(user);
                        state.next = Some(Active::Main(MainView::new()));
                    }
                    Err(_e) => {}
                }),
            ))
        },
        popup: Popup::new(exists),
    };
    let mut term = ratatui::init();
    let mut err = None;

    app.state.update_rooms();

    loop {
        if let Err(e) = term.draw(|frame| frame.render_widget(&app, frame.area())) {
            err = Some(e);
            break;
        }

        match event::read() {
            Ok(event::Event::Key(ev)) => {
                if ev.kind == KeyEventKind::Press {
                    app.on_press(ev.code);

                    if app.state.quit {
                        break;
                    }
                }
            }
            Err(e) => {
                err = Some(e);
                break;
            }
            _ => {}
        }

        if let Some(next) = app.state.next.take() {
            app.active = next;
        }
    }

    ratatui::restore();

    if let Some(e) = err {
        println!("{e}");
    }
}
