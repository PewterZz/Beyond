//! Persistent command history — stored as one entry per line in data_dir/history.
//! Navigation mirrors shell behaviour: Up moves backward, Down moves forward,
//! Down past the newest entry restores the in-progress draft.

use std::path::PathBuf;

const MAX_ENTRIES: usize = 10_000;

pub struct CommandHistory {
    /// All entries, oldest first.
    entries: Vec<String>,
    /// Current navigation position. `None` means the user is at the live input.
    pos: Option<usize>,
    /// Saved draft text so Down can restore it when the user navigates back.
    draft: String,
    /// Path to the history file.
    path: PathBuf,
}

impl CommandHistory {
    /// Load from `data_dir/history`, creating the file if it does not exist.
    pub fn load(data_dir: &std::path::Path) -> Self {
        let path = data_dir.join("history");
        let entries = if path.exists() {
            std::fs::read_to_string(&path)
                .unwrap_or_default()
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect()
        } else {
            vec![]
        };
        Self {
            entries,
            pos: None,
            draft: String::new(),
            path,
        }
    }

    /// Push a new entry. Skips empty strings and consecutive duplicates.
    /// Saves to disk immediately.
    pub fn push(&mut self, entry: String) {
        if entry.is_empty() {
            return;
        }
        if self.entries.last().map(|e| e == &entry).unwrap_or(false) {
            // Duplicate of the last entry — don't store again but reset position.
            self.reset();
            return;
        }
        self.entries.push(entry);
        // Trim to cap.
        if self.entries.len() > MAX_ENTRIES {
            self.entries.drain(0..self.entries.len() - MAX_ENTRIES);
        }
        self.reset();
        self.save();
    }

    /// Move backward (older). Returns the entry to display, or None if already at oldest.
    pub fn up(&mut self, current_text: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        match self.pos {
            None => {
                // Save draft before leaving the live position.
                self.draft = current_text.to_string();
                self.pos = Some(self.entries.len() - 1);
            }
            Some(0) => return Some(&self.entries[0]), // Already at oldest — no change.
            Some(i) => self.pos = Some(i - 1),
        }
        self.pos.map(|i| self.entries[i].as_str())
    }

    /// Move forward (newer). Returns the entry to display, or None when back at live input.
    pub fn down(&mut self) -> Option<&str> {
        match self.pos {
            None => None, // Already at live input.
            Some(i) if i + 1 >= self.entries.len() => {
                self.pos = None;
                None // Signal: restore draft
            }
            Some(i) => {
                self.pos = Some(i + 1);
                Some(&self.entries[i + 1])
            }
        }
    }

    /// The saved draft text (used to restore live input on Down past newest).
    pub fn draft(&self) -> &str {
        &self.draft
    }

    /// Reset navigation position (call after submit or any non-navigation edit).
    pub fn reset(&mut self) {
        self.pos = None;
        self.draft = String::new();
    }

    fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let content = self.entries.join("\n") + "\n";
        let _ = std::fs::write(&self.path, content);
    }
}
