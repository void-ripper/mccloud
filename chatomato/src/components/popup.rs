use ratatui::{layout::Rect, widgets::Widget};

pub struct Popup {
    show: bool,
}

impl Widget for Popup {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
    }
}
