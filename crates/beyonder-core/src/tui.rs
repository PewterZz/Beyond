//! TUI cell type shared between terminal emulation and GPU rendering.

/// A single rendered cell in a TUI screen grid.
#[derive(Debug, Clone)]
pub struct TuiCell {
    pub ch: char,
    /// Foreground RGB color (linear, 0.0–1.0).
    pub fg: [f32; 3],
    /// Background RGB color. `None` means default background (transparent/Base).
    pub bg: Option<[f32; 3]>,
    pub bold: bool,
    pub italic: bool,
}
