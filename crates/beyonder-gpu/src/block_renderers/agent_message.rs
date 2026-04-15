//! Renderer for AgentMessage blocks.

use beyonder_core::Block;
use crate::pipeline::RectInstance;

pub fn render_agent_message(
    _block: &Block,
    _x: f32,
    _y: f32,
    _width: f32,
    _height: f32,
    _scale: f32,
    _rects: &mut Vec<RectInstance>,
) {
    // Agent messages render as plain text — no background decoration.
}
