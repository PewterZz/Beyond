# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Beyonder is an AI-native terminal written in Rust. It replaces the traditional scroll buffer with a **block-oriented** model: every piece of content (shell output, agent messages, approvals, diffs, tool calls) is a persistent `Block` with provenance. Agents are first-class, long-lived processes with capability sets and resource limits, supervised analogously to OS processes. Rendering is GPU-accelerated (wgpu + glyphon) inside a single winit window; there is no TTY.

## Build & Run

```bash
cargo build                      # debug build of workspace
cargo run                        # launch the app (opens the wgpu window)
cargo build --release            # release (LTO, codegen-units=1)
cargo check -p <crate>           # fast type-check a single crate
cargo test                       # run all tests
cargo test -p beyonder-core      # tests for a single crate
cargo test <name> -- --nocapture # single test, show stdout
cargo clippy --workspace --all-targets
cargo fmt --all
```

Logging is via `tracing`. Control with `RUST_LOG` (default: `beyonder=info,wgpu_core=warn,wgpu_hal=warn`). Logs go to **stderr**; redirect with `cargo run 2> run.log` (stdout is buffered and will hide hangs).

Three LLM providers are supported: **Ollama** (local + Turbo), **llama.cpp** (`llama-server`), and **Apple MLX** (`mlx_lm.server`). The active provider is set in `config.toml` or at runtime with `/provider <name>`. Ensure the relevant server is running before spawning agents.

## Workspace Layout

Workspace root builds the `beyonder` binary (`src/main.rs`) which is a thin winit `ApplicationHandler` that owns the tokio runtime and delegates to `beyonder-ui::App`. The crates form a layered dependency graph:

- **beyonder-core** — pure data model. `Block`/`BlockId`/`BlockKind`/`BlockContent`, `AgentId`/`AgentInfo`/`AgentState`, `SessionId`, `CapabilitySet`, `ProvenanceChain`, `TuiCell`. No I/O; everything else depends on it.
- **beyonder-store** — SQLite persistence (`rusqlite`, bundled). `BlockStore`, `SessionStore`, migrations. The `Store` wraps a single `Connection`.
- **beyonder-terminal** — PTY management (`portable-pty`) and terminal emulation (`alacritty_terminal`). `PtySession`, `TermGrid`, `BlockBuilder` turns raw PTY output into shell blocks via OSC-133 shell hooks.
- **beyonder-acp** — Agent Client Protocol: messages, transport, `AcpClient`. Streaming events from agent backends.
- **beyonder-runtime** — `AgentSupervisor` spawns and monitors agents; `CapabilityBroker` gates tool use; `tools::` registry executes tool calls; `provider::` holds the `AgentBackend` trait, `OllamaBackend` (NDJSON), and `OpenAICompatBackend` (SSE, used by both llama.cpp and MLX). Runtime is where the async turn-drivers live (one tokio task per agent, driven via `mpsc` command channels).
- **beyonder-gpu** — wgpu 24 renderer. `Renderer` owns the device/queue/surface and text atlas (glyphon 0.8). `Viewport` handles scrolling. Per-block renderers live in `block_renderers/` (agent_message, approval, shell_block, etc.). The input bar has a **dynamic height**: it grows up to `MAX_INPUT_LINES = 4` visual lines as the user types, then scrolls to keep the cursor visible. `Renderer::compute_bar_state()` recalculates `computed_bar_h` and `input_scroll_px` once per frame; all bar layout uses `computed_bar_h` rather than the constant.
- **beyonder-ui** — the `App` struct: wires supervisor, store, renderer, input editor, history, mode detector, commands. `App::tick()` (called from `about_to_wait`) drains supervisor/broker events so streaming works even when the window is occluded. `App::handle_window_event` + `App::render` are called from `window_event`.
- **beyonder-config** — `BeyonderConfig` + `ProviderConfig` enum loaded from TOML.

## Runtime Loop (important)

`src/main.rs` runs a custom winit loop with `ControlFlow::WaitUntil(+8ms)`:
1. `resumed` → create window, `App::new(window, config).await` under tokio.
2. `window_event` → `app.handle_window_event(&event).await`; on `RedrawRequested` → `app.render()`.
3. `about_to_wait` → `app.tick().await` (drain events, advance streaming state) then `window.request_redraw()`.

Do **not** move state-advancement into `RedrawRequested` — macOS suppresses redraws for hidden windows and streaming agent output would stall. Keep `tick()` in `about_to_wait`.

## Block / Agent Model

- A `Block` has `id` (ULID), `kind`, optional `parent_id` / `agent_id`, `session_id`, timestamps, `status` (Pending/…), `content`, and a `ProvenanceChain`. Blocks are immutable append-only except for status/`updated_at`; new content = new block with `parent_id`.
- Agents have `AgentState` (Spawning/…), `CapabilitySet` (what tools they may invoke), and `ResourceLimits`. The `AgentSupervisor` owns an `AgentHandle` per agent with an `mpsc::UnboundedSender<AgentCmd>` (`Prompt` / `ResetConversation`). Events flow back via `SupervisorEvent`.
- Tool execution goes through `CapabilityBroker` — never bypass it.

## Provider Configuration

`ProviderConfig` is a tagged TOML enum in `beyonder-config/src/config.rs`. Three variants:

```toml
# Ollama (default)
[provider]
kind = "ollama"
base_url = "http://localhost:11434"   # optional; cloud: "https://ollama.com"
api_key_env = "OLLAMA_API_KEY"        # optional; omit for local

# llama.cpp — start server with: llama-server -m model.gguf --jinja -c 8192
[provider]
kind = "llama_cpp"
base_url = "http://127.0.0.1:8080/v1"

# Apple MLX — requires mlx-lm >= 0.19; start with: mlx_lm.server --model <id>
[provider]
kind = "mlx"
base_url = "http://127.0.0.1:8080/v1"
```

Switch at runtime with `/provider ollama|llama_cpp|mlx` (saves to config). Switch model with `/model <name>`. Both take effect on the next agent spawn — use `/clear` or restart to respawn with new settings if an agent is already running.

`OpenAICompatBackend` (`provider/openai_compat.rs`) handles both llama.cpp and MLX. Key differences from Ollama: SSE framing, tool-call arguments arrive as string fragments that are reassembled before JSON parsing, tool result messages use `tool_call_id` instead of `name`.

## Input Editor & Keyboard Shortcuts

`InputEditor` (`beyonder-ui/src/input_editor.rs`) is a single-line UTF-8 editor with cursor and history. It supports:

- **Editing**: `Cmd+A` (select-all), `Cmd+X` (cut), `Cmd+C` (copy selected), `Cmd+V` (paste from clipboard or bracketed paste into PTY), `Ctrl+K` (kill to end), `Ctrl+U` (kill to start), `Ctrl+W` / `Alt+Backspace` (delete word backward).
- **Navigation**: `←`/`→`, `Cmd+←`/`Cmd+→` (home/end), `Alt+←`/`Alt+→` (word left/right), `↑`/`↓` (history).
- **Clipboard**: `arboard` for the system clipboard. OSC 52 passthrough (`\x1b]52;...`) lets TUI apps (nvim, etc.) read/write the clipboard; responses are written back to the PTY in `App::tick()`.
- **Bracketed paste**: `\x1b[200~{text}\x1b[201~` is sent to the active PTY when paste is triggered in TUI mode.
- The `all_selected` flag on `InputEditor` signals "select-all active"; the renderer renders the input in Catppuccin Blue with a block cursor. Any subsequent insert/delete replaces the entire contents.

The input bar height is dynamic (see beyonder-gpu above). It grows by one `font_size * 1.4` line per wrapped visual line, up to `MAX_INPUT_LINES = 4`, then scrolls. The viewport above the bar adjusts automatically.

## Conventions

- Use the existing `beyonder-core` IDs (`BlockId`, `AgentId`, `SessionId`) — all ULID-backed. Don't invent new ID types.
- Workspace dependencies are declared once in root `Cargo.toml`; reference them as `foo = { workspace = true }` in crate manifests.
- `dev` profile uses `opt-level = 1` (beware: debug builds are slower to compile than vanilla but much faster at runtime — needed for the render loop).
