//! Theme system: semantic color slots + built-in palettes.
//! Palettes mirror Catppuccin's naming so role → color stays stable across variants.
//!
//! Renderer consumes the `Theme` struct through `BeyonderConfig::resolved_theme()`.
//! Users switch palettes with `/theme <name>` at runtime or by editing the TOML.
//!
//! Unknown names fall back to `mocha`.

use serde::{Deserialize, Serialize};

/// A resolved theme — raw RGB triples in `[0, 255]`.
/// Named after Catppuccin's accent slots so the same code works across variants.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Theme {
    pub name: &'static str,
    // Surfaces: bg (window), surface (block), surface_alt (input bar), border.
    // Stored as [r,g,b,a] in [0,1] because wgpu rects want this format directly.
    pub bg: [f32; 4],
    pub surface: [f32; 4],
    pub surface_alt: [f32; 4],
    pub border: [f32; 4],
    // Text slots — [r,g,b] in [0,255] for GlyphColor::rgb().
    pub text: [u8; 3],
    pub subtext: [u8; 3],
    pub muted: [u8; 3],
    // Accents.
    pub red: [u8; 3],
    pub peach: [u8; 3],
    pub yellow: [u8; 3],
    pub green: [u8; 3],
    pub teal: [u8; 3],
    pub sky: [u8; 3],
    pub sapphire: [u8; 3],
    pub blue: [u8; 3],
    pub lavender: [u8; 3],
    pub mauve: [u8; 3],
    pub pink: [u8; 3],
}

impl Default for Theme {
    fn default() -> Self {
        MOCHA
    }
}

/// Look up a theme by name. Returns `MOCHA` for unknown names.
pub fn theme_by_name(name: &str) -> Theme {
    match name.to_ascii_lowercase().as_str() {
        "mocha" | "catppuccin-mocha" => MOCHA,
        "macchiato" | "catppuccin-macchiato" => MACCHIATO,
        "frappe" | "catppuccin-frappe" => FRAPPE,
        "latte" | "catppuccin-latte" => LATTE,
        _ => MOCHA,
    }
}

/// All known theme names (for /theme tab-completion and /help).
pub const BUILTIN_THEMES: &[&str] = &["mocha", "macchiato", "frappe", "latte"];

pub const MOCHA: Theme = Theme {
    name: "mocha",
    bg:          [0.118, 0.118, 0.180, 1.0], // Base   #1e1e2e
    surface:     [0.180, 0.180, 0.251, 1.0], // Surface0 #313244
    surface_alt: [0.098, 0.098, 0.145, 1.0], // Mantle #181825
    border:      [0.271, 0.278, 0.353, 1.0], // Surface1 #45475a
    text:        [205, 214, 244],            // Text #cdd6f4
    subtext:     [166, 173, 200],            // between Subtext1 / Overlay2
    muted:       [108, 112, 134],            // Overlay0 #6c7086
    red:         [243, 139, 168],            // #f38ba8
    peach:       [250, 179, 135],            // #fab387
    yellow:      [249, 226, 175],            // #f9e2af
    green:       [166, 227, 161],            // #a6e3a1
    teal:        [148, 226, 213],            // #94e2d5
    sky:         [137, 220, 235],            // #89dceb
    sapphire:    [116, 199, 236],            // #74c7ec
    blue:        [137, 180, 250],            // #89b4fa
    lavender:    [180, 190, 254],            // #b4befe
    mauve:       [203, 166, 247],            // #cba6f7
    pink:        [245, 194, 231],            // #f5c2e7
};

pub const MACCHIATO: Theme = Theme {
    name: "macchiato",
    bg:          [0.141, 0.149, 0.212, 1.0], // Base   #24273a
    surface:     [0.227, 0.239, 0.314, 1.0], // Surface0 #363a4f
    surface_alt: [0.110, 0.118, 0.180, 1.0], // Mantle #1e2030
    border:      [0.298, 0.314, 0.388, 1.0],
    text:        [202, 211, 245],
    subtext:     [165, 173, 203],
    muted:       [110, 115, 141],
    red:         [237, 135, 150],
    peach:       [245, 169, 127],
    yellow:      [238, 212, 159],
    green:       [166, 218, 149],
    teal:        [139, 213, 202],
    sky:         [145, 215, 227],
    sapphire:    [125, 196, 228],
    blue:        [138, 173, 244],
    lavender:    [183, 189, 248],
    mauve:       [198, 160, 246],
    pink:        [245, 189, 230],
};

pub const FRAPPE: Theme = Theme {
    name: "frappe",
    bg:          [0.188, 0.196, 0.267, 1.0], // Base   #303446
    surface:     [0.255, 0.267, 0.329, 1.0],
    surface_alt: [0.153, 0.161, 0.224, 1.0],
    border:      [0.333, 0.345, 0.412, 1.0],
    text:        [198, 208, 245],
    subtext:     [165, 173, 206],
    muted:       [115, 121, 148],
    red:         [231, 130, 132],
    peach:       [239, 159, 118],
    yellow:      [229, 200, 144],
    green:       [166, 209, 137],
    teal:        [129, 200, 190],
    sky:         [153, 209, 219],
    sapphire:    [133, 193, 220],
    blue:        [140, 170, 238],
    lavender:    [186, 187, 241],
    mauve:       [202, 158, 230],
    pink:        [244, 184, 228],
};

pub const LATTE: Theme = Theme {
    name: "latte",
    bg:          [0.937, 0.933, 0.937, 1.0], // Base   #eff1f5 — light
    surface:     [0.878, 0.886, 0.922, 1.0],
    surface_alt: [0.957, 0.961, 0.973, 1.0],
    border:      [0.769, 0.788, 0.839, 1.0],
    text:        [76, 79, 105],
    subtext:     [108, 111, 133],
    muted:       [156, 160, 176],
    red:         [210, 15, 57],
    peach:       [254, 100, 11],
    yellow:      [223, 142, 29],
    green:       [64, 160, 43],
    teal:        [23, 146, 153],
    sky:         [4, 165, 229],
    sapphire:    [32, 159, 181],
    blue:        [30, 102, 245],
    lavender:    [114, 135, 253],
    mauve:       [136, 57, 239],
    pink:        [234, 118, 203],
};
