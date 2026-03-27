use crossterm::event::KeyCode;

use crate::data::AppSnapshot;

pub struct App {
    /// Index of the currently selected channel row.
    pub selected: usize,
    /// Whether the polling / UI updates are paused.
    pub paused: bool,
    /// Whether to show the throughput graph panel.
    pub show_graph: bool,
    /// Most recent snapshot from the poller.
    pub snapshot: AppSnapshot,
    /// Set to true when the user presses q / Esc.
    pub should_quit: bool,
}

impl App {
    pub fn new(initial: AppSnapshot) -> Self {
        Self {
            selected: 0,
            paused: false,
            show_graph: true,
            snapshot: initial,
            should_quit: false,
        }
    }

    /// Replace the snapshot (called on each poll tick, unless paused).
    pub fn update(&mut self, snap: AppSnapshot) {
        if !self.paused {
            // Keep selection in bounds if channels were removed.
            if !snap.channels.is_empty() {
                self.selected = self.selected.min(snap.channels.len() - 1);
            }
            self.snapshot = snap;
        }
    }

    pub fn handle_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('p') => self.paused = !self.paused,
            KeyCode::Char('g') => self.show_graph = !self.show_graph,
            KeyCode::Down | KeyCode::Char('j') => {
                let len = self.snapshot.channels.len();
                if len > 0 {
                    self.selected = (self.selected + 1).min(len - 1);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
            }
            _ => {}
        }
    }
}
