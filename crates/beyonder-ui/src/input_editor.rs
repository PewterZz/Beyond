//! Single-line input editor with cursor and submission history.
//! Handles keyboard events and produces text for routing by mode_detector.

#[derive(Debug, Clone, Default)]
pub struct InputEditor {
    pub text: String,
    pub cursor: usize,
    history: Vec<String>,
    /// None = live input; Some(i) = browsing history at index i (0 = most recent).
    history_idx: Option<usize>,
    /// Stashed live text while browsing history.
    draft: String,
}

impl InputEditor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, ch: char) {
        self.history_idx = None;
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    pub fn delete_backward(&mut self) {
        self.history_idx = None;
        if self.cursor == 0 {
            return;
        }
        let prev = self.prev_char_boundary();
        self.text.drain(prev..self.cursor);
        self.cursor = prev;
    }

    pub fn delete_forward(&mut self) {
        self.history_idx = None;
        if self.cursor >= self.text.len() {
            return;
        }
        let next = self.next_char_boundary();
        self.text.drain(self.cursor..next);
    }

    pub fn move_left(&mut self) {
        self.cursor = self.prev_char_boundary();
    }

    pub fn move_right(&mut self) {
        self.cursor = self.next_char_boundary();
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.text.len();
    }

    /// Walk backward through history (older). Returns true if the display changed.
    pub fn history_prev(&mut self) -> bool {
        if self.history.is_empty() {
            return false;
        }
        let next_idx = match self.history_idx {
            None => {
                self.draft = self.text.clone();
                0
            }
            Some(i) if i + 1 < self.history.len() => i + 1,
            _ => return false,
        };
        self.history_idx = Some(next_idx);
        let entry = self.history[self.history.len() - 1 - next_idx].clone();
        self.text = entry;
        self.cursor = self.text.len();
        true
    }

    /// Walk forward through history (newer / back to draft). Returns true if the display changed.
    pub fn history_next(&mut self) -> bool {
        match self.history_idx {
            None => false,
            Some(0) => {
                self.history_idx = None;
                self.text = self.draft.clone();
                self.cursor = self.text.len();
                true
            }
            Some(i) => {
                let next_idx = i - 1;
                self.history_idx = Some(next_idx);
                let entry = self.history[self.history.len() - 1 - next_idx].clone();
                self.text = entry;
                self.cursor = self.text.len();
                true
            }
        }
    }

    /// Push a submitted entry into history (dedup consecutive identical entries).
    pub fn push_history(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        if self.history.last().map(|s| s == &text).unwrap_or(false) {
            return;
        }
        self.history.push(text);
    }

    /// Replace the editor content and move cursor to end.
    pub fn set_text(&mut self, text: String) {
        self.history_idx = None;
        self.cursor = text.len();
        self.text = text;
    }

    /// Take the current text and clear the editor (submit).
    pub fn submit(&mut self) -> String {
        self.history_idx = None;
        self.draft.clear();
        let text = std::mem::take(&mut self.text);
        self.cursor = 0;
        text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    fn prev_char_boundary(&self) -> usize {
        if self.cursor == 0 {
            return 0;
        }
        let mut i = self.cursor - 1;
        while !self.text.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    fn next_char_boundary(&self) -> usize {
        if self.cursor >= self.text.len() {
            return self.text.len();
        }
        let mut i = self.cursor + 1;
        while i < self.text.len() && !self.text.is_char_boundary(i) {
            i += 1;
        }
        i
    }
}
