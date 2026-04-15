//! Wraps alacritty_terminal::Term — maintains live terminal screen state.
//! Used to track TUI apps (alternate-screen mode) and extract the grid for rendering.

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, TermMode};
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, CursorShape, NamedColor, Processor, Rgb};
use alacritty_terminal::Term;
use beyonder_core::TuiCell;

struct GridSize {
    cols: usize,
    rows: usize,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// Live terminal screen state backed by alacritty_terminal.
pub struct TermGrid {
    term: Term<VoidListener>,
    processor: Processor,
    pub cols: usize,
    pub rows: usize,
}

impl TermGrid {
    pub fn new(cols: usize, rows: usize) -> Self {
        let config = Config::default();
        let size = GridSize { cols, rows };
        let term = Term::new(config, &size, VoidListener);
        Self { term, processor: Processor::new(), cols, rows }
    }

    /// Feed raw PTY bytes into the terminal state machine.
    pub fn feed(&mut self, bytes: &[u8]) {
        // Dump raw PTY bytes (escaped) to BEYONDER_PTY_LOG when set — useful for
        // diagnosing glyph/escape-sequence mismatches without fighting stderr.
        if let Ok(path) = std::env::var("BEYONDER_PTY_LOG") {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
                let mut out = String::new();
                for &b in bytes {
                    match b {
                        0x1b => out.push_str("\\e"),
                        0x07 => out.push_str("\\a"),
                        0x0a => out.push_str("\\n\n"),
                        0x0d => out.push_str("\\r"),
                        0x09 => out.push_str("\\t"),
                        0x20..=0x7e => out.push(b as char),
                        _ => out.push_str(&format!("\\x{:02x}", b)),
                    }
                }
                let _ = f.write_all(out.as_bytes());
            }
        }
        self.processor.advance(&mut self.term, bytes);
    }

    /// Reset to a blank screen, keeping the same dimensions.
    /// Call this when a new command starts so the live view only shows
    /// output from that command, not accumulated history.
    pub fn reset(&mut self) {
        let config = Config::default();
        let size = GridSize { cols: self.cols, rows: self.rows };
        self.term = Term::new(config, &size, VoidListener);
        self.processor = Processor::new();
    }

    /// Resize the terminal grid to new dimensions.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        if self.cols == cols && self.rows == rows {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        let size = GridSize { cols, rows };
        self.term.resize(size);
    }

    /// Scroll the display back into history by `delta` lines (positive = up into
    /// scrollback, negative = down toward the live screen). No-op in alt-screen
    /// because alt-screen keeps no history. Clamped by alacritty to the grid's
    /// `history_size()`.
    pub fn scroll_display(&mut self, delta: i32) {
        self.term.scroll_display(Scroll::Delta(delta));
    }

    /// Reset the display offset to the live screen (scroll all the way down).
    pub fn scroll_to_bottom(&mut self) {
        self.term.scroll_display(Scroll::Bottom);
    }

    /// Current display offset in lines above the live screen (0 = live, >0 = in history).
    pub fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    /// True when an alternate-screen TUI app is active.
    pub fn tui_active(&self) -> bool {
        self.term.mode().contains(TermMode::ALT_SCREEN)
    }

    /// True when app-cursor mode is active (TUI apps often set this).
    /// Affects arrow key escape sequences sent to PTY.
    pub fn app_cursor_mode(&self) -> bool {
        self.term.mode().contains(TermMode::APP_CURSOR)
    }

    /// Current cursor shape as a u8: 0=block, 1=beam, 2=underline.
    /// Maps from the alacritty CursorShape enum for renderer consumption.
    pub fn cursor_shape_code(&self) -> u8 {
        match self.term.cursor_style().shape {
            CursorShape::Beam => 1,
            CursorShape::Underline => 2,
            _ => 0, // Block, HollowBlock, Hidden → block render
        }
    }

    /// Current cursor position as (row, col). Clamped to grid bounds.
    pub fn cursor_pos(&self) -> (usize, usize) {
        let grid = self.term.grid();
        let row = grid.cursor.point.line.0.max(0) as usize;
        let col = grid.cursor.point.column.0;
        let rows = self.term.screen_lines();
        let cols = self.term.columns();
        // Live row is relative to the live screen (0..rows). When the user
        // scrolls back into history the live cursor is below the viewport, so
        // shift the returned row by the current display_offset. Rows >= screen
        // height naturally fall outside the renderer's draw band and the
        // cursor disappears — matches how real terminals behave.
        let offset = grid.display_offset();
        let visible = row.saturating_add(offset);
        (visible.min(rows.saturating_add(offset)), col.min(cols.saturating_sub(1)))
    }

    /// Extract the full screen grid as TuiCells.
    pub fn cell_grid(&self) -> Vec<Vec<TuiCell>> {
        let rows = self.term.screen_lines();
        let cols = self.term.columns();
        let grid = self.term.grid();
        // Shift every read upward by display_offset so scrollback is visible.
        // 0 = live screen; positive = N lines back into history.
        let offset = grid.display_offset() as i32;
        let mut result = Vec::with_capacity(rows);

        for row_idx in 0..rows {
            let line = Line(row_idx as i32 - offset);
            let mut row_cells = Vec::with_capacity(cols);
            for col_idx in 0..cols {
                let col = Column(col_idx);
                let cell = &grid[line][col];

                // INVERSE (reverse video): swap fg↔bg. Used by nvim for cursor,
                // statusline, visual selection, search highlights, etc.
                let inverse = cell.flags.contains(Flags::INVERSE);
                let (effective_fg, effective_bg) = if inverse {
                    (&cell.bg, &cell.fg)
                } else {
                    (&cell.fg, &cell.bg)
                };
                let fg = resolve_color(effective_fg);
                // When inverse is set, the background is always the (swapped) fg color —
                // it must always be drawn even if it resolves to a named/indexed color.
                let bg = if inverse {
                    Some(resolve_color(effective_bg))
                } else if is_default_bg(effective_bg) {
                    None
                } else {
                    Some(resolve_color(effective_bg))
                };
                // HIDDEN: character is invisible — render as space.
                let ch = if cell.flags.contains(Flags::HIDDEN) { ' ' } else { cell.c };

                row_cells.push(TuiCell {
                    ch,
                    fg,
                    bg,
                    bold: cell.flags.contains(Flags::BOLD),
                    italic: cell.flags.contains(Flags::ITALIC),
                });
            }
            result.push(row_cells);
        }
        result
    }
}

fn is_default_bg(color: &AnsiColor) -> bool {
    matches!(color, AnsiColor::Named(NamedColor::Background))
}

fn resolve_color(color: &AnsiColor) -> [f32; 3] {
    match color {
        AnsiColor::Spec(Rgb { r, g, b }) => {
            [*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0]
        }
        AnsiColor::Named(named) => named_to_rgb(*named),
        AnsiColor::Indexed(idx) => indexed_to_rgb(*idx),
    }
}

/// Catppuccin Mocha terminal palette — matches the UI theme so rendered
/// app colors look consistent with the rest of the Beyonder interface.
fn named_to_rgb(color: NamedColor) -> [f32; 3] {
    match color {
        NamedColor::Black         => [0.271, 0.278, 0.353], // #45475a Surface1
        NamedColor::Red           => [0.953, 0.545, 0.659], // #f38ba8 Red
        NamedColor::Green         => [0.651, 0.890, 0.631], // #a6e3a1 Green
        NamedColor::Yellow        => [0.976, 0.886, 0.686], // #f9e2af Yellow
        NamedColor::Blue          => [0.537, 0.706, 0.980], // #89b4fa Blue
        NamedColor::Magenta       => [0.961, 0.761, 0.906], // #f5c2e7 Pink
        NamedColor::Cyan          => [0.580, 0.886, 0.835], // #94e2d5 Teal
        NamedColor::White         => [0.729, 0.761, 0.871], // #bac2de Subtext1
        NamedColor::BrightBlack   => [0.345, 0.357, 0.439], // #585b70 Surface2
        NamedColor::BrightRed     => [0.953, 0.545, 0.659], // #f38ba8 Red
        NamedColor::BrightGreen   => [0.651, 0.890, 0.631], // #a6e3a1 Green
        NamedColor::BrightYellow  => [0.976, 0.886, 0.686], // #f9e2af Yellow
        NamedColor::BrightBlue    => [0.537, 0.706, 0.980], // #89b4fa Blue
        NamedColor::BrightMagenta => [0.961, 0.761, 0.906], // #f5c2e7 Pink
        NamedColor::BrightCyan    => [0.580, 0.886, 0.835], // #94e2d5 Teal
        NamedColor::BrightWhite   => [0.651, 0.678, 0.784], // #a6adc8 Subtext0
        NamedColor::Foreground    => [0.804, 0.839, 0.957], // #cdd6f4 Text
        NamedColor::Background    => [0.118, 0.118, 0.180], // #1e1e2e Base
        _                         => [0.804, 0.839, 0.957], // #cdd6f4 Text
    }
}

fn indexed_to_rgb(idx: u8) -> [f32; 3] {
    match idx {
        0  => named_to_rgb(NamedColor::Black),
        1  => named_to_rgb(NamedColor::Red),
        2  => named_to_rgb(NamedColor::Green),
        3  => named_to_rgb(NamedColor::Yellow),
        4  => named_to_rgb(NamedColor::Blue),
        5  => named_to_rgb(NamedColor::Magenta),
        6  => named_to_rgb(NamedColor::Cyan),
        7  => named_to_rgb(NamedColor::White),
        8  => named_to_rgb(NamedColor::BrightBlack),
        9  => named_to_rgb(NamedColor::BrightRed),
        10 => named_to_rgb(NamedColor::BrightGreen),
        11 => named_to_rgb(NamedColor::BrightYellow),
        12 => named_to_rgb(NamedColor::BrightBlue),
        13 => named_to_rgb(NamedColor::BrightMagenta),
        14 => named_to_rgb(NamedColor::BrightCyan),
        15 => named_to_rgb(NamedColor::BrightWhite),
        16..=231 => {
            let i = idx - 16;
            let r_idx = i / 36;
            let g_idx = (i % 36) / 6;
            let b_idx = i % 6;
            let to_f = |v: u8| if v == 0 { 0.0 } else { (55 + v * 40) as f32 / 255.0 };
            [to_f(r_idx), to_f(g_idx), to_f(b_idx)]
        }
        232..=255 => {
            let v = (8 + (idx - 232) as u16 * 10) as f32 / 255.0;
            [v, v, v]
        }
    }
}
