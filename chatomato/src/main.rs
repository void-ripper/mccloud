use components::{create_user::CreateUser, main::MainView, popup::Popup, Component};
use config::Config;
use db::Database;
use indexmap::IndexSet;
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

pub(crate) struct State {
    db: Database,
    rooms: IndexSet<String>,
    quit: bool,
}

impl State {
    pub(crate) fn update_rooms(&mut self) {
        let rooms = self.db.list_rooms().unwrap();
        self.rooms.extend(rooms.iter().map(|r| r.name.clone()));
    }
}

struct App {
    state: State,
    main: MainView,
    popup: Popup,
    create_user: CreateUser,
}

impl App {
    fn on_press(&mut self, ev: KeyCode) {
        self.main.on_press(&mut self.state, ev);
        self.popup.on_press(&mut self.state, ev);
        self.create_user.on_press(&mut self.state, ev);
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        self.main.render(area, buf);

        self.create_user.render(area, buf);
        self.popup.render(area, buf);
    }
}

fn main() {
    let cfg = Config {
        addr: "0.0.0.0:29092".into(),
        data: "data".into(),
    };
    let prikey = cfg.data.join("private.key");
    let exists = prikey.exists();
    let db = Database::new(cfg).unwrap();
    let mut app = App {
        state: State {
            rooms: IndexSet::new(),
            db,
            quit: false,
        },
        main: MainView::new(),
        popup: Popup::new(exists),
        create_user: CreateUser::new(!exists),
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
    }

    ratatui::restore();

    if let Some(e) = err {
        println!("{e}");
    }
}
