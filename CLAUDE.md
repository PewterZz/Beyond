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
- **beyonder-config** — `BeyonderConfig` + `ProviderConfig` enum loaded from TOML. Theme is a named string (`"mocha" | "macchiato" | "frappe" | "latte"`) resolved to a `Theme` palette via `theme_by_name`. Config lives at `~/.config/beyond/config.toml` (honours `$XDG_CONFIG_HOME`) and is **hot-reloaded** via `notify` watching the parent dir — theme changes apply live, provider/model changes apply on next agent spawn, font changes warn "requires restart".

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

`InputEditor` (`beyonder-ui/src/input_editor.rs`) is a UTF-8 editor with cursor and history. Newlines (`\n`) are allowed in the buffer (inserted via `Shift+Enter`). It supports:

- **Editing**: `Cmd+A` (select-all), `Cmd+X` (cut), `Cmd+C` (copy selected), `Cmd+V` (paste from clipboard or bracketed paste into PTY), `Ctrl+K` (kill to end), `Ctrl+U` (kill to start), `Ctrl+W` / `Alt+Backspace` (delete word backward).
- **Navigation**: `←`/`→`, `Cmd+←`/`Cmd+→` (home/end), `Alt+←`/`Alt+→` (word left/right), `↑`/`↓` (history).
- **Submit vs newline**: `Enter` submits; `Shift+Enter` inserts `\n` for a true multi-line prompt.
- **Clipboard**: `arboard` for the system clipboard. OSC 52 passthrough (`\x1b]52;...`) lets TUI apps (nvim, etc.) read/write the clipboard; responses are written back to the PTY in `App::tick()`.
- **Bracketed paste**: `\x1b[200~{text}\x1b[201~` is sent to the active PTY when paste is triggered in TUI mode.
- The `all_selected` flag on `InputEditor` signals "select-all active"; the renderer renders the input in Catppuccin Blue with a block cursor. Any subsequent insert/delete replaces the entire contents.

The input bar height is dynamic (see beyonder-gpu above). It grows by one `font_size * 1.4` line per visual line (wrap **or** explicit `\n`), up to `MAX_INPUT_LINES = 4`, then scrolls. The viewport above the bar adjusts automatically.

**Input scroll model** — `input_scroll_px` is an independent viewport offset within the input text area:
- Mouse wheel over the input bar calls `Renderer::scroll_input(delta)` — scrolls the input text freely (lets you see the top of a long pasted message).
- Mouse wheel over the block stream calls `Renderer::scroll(delta)` — scrolls blocks (see below).
- Any keystroke that edits or moves the cursor calls `Renderer::snap_input_scroll_to_cursor()` to bring the cursor back into view. This is called from every early-return branch in `App::handle_key_event` that mutates input (paste, cut, kill, word/home/end nav, etc.) — keep this invariant when adding new shortcuts.
- `compute_bar_state()` only clamps `input_scroll_px` to `[0, max_scroll]`; it does **not** cursor-follow. Cursor-follow is exclusively `snap_input_scroll_to_cursor`'s job.

**Block stream scroll model** — `Viewport::pinned_to_bottom` (`beyonder-gpu/src/viewport.rs`) drives auto-follow:
- `scroll_to_bottom()` sets it `true`; `scroll_to_top()` sets it `false`; `scroll(delta)` sets it based on whether the resulting offset is within 1 px of `max_scroll`.
- The per-frame auto-snap in `Renderer::render` (`if running_block_idx.is_some() && viewport.pinned_to_bottom { scroll_to_bottom() }`) only fires while pinned — so scrolling up during streaming sticks.
- `App::add_block` and `App::push_text_block` only call `scroll_to_bottom()` when `pinned_to_bottom` is already true — new agent/shell/approval blocks don't yank the user back down mid-read.
- Explicit user actions (submitting a prompt via `push_user_block` / `push_pending_agent_block`, and `/clear`) unconditionally re-pin to bottom.

## TUI / Full-Screen App Rendering

Beyonder hosts TUIs (nvim, htop, `claude`) in the same block stream by switching into a **full-window cell grid** mode. Two triggers:
1. `term_grid.tui_active()` — alt-screen apps (sets `DECSET 1049`).
2. Name-matched interactive CLIs in `App::render` — currently `claude` / `claude-code`, which don't use alt-screen but still take over the window. When either is true, `Renderer::tui_active = true`.

Important invariants:
- **PTY sizing**: when TUI is active (either trigger), the PTY is resized to `Renderer::tui_grid_size()` (full window minus `TUI_PAD`). Both the `Resized` handler (`app.rs`) *and* the per-tick transition detector must OR-in the `interactive_cli` check — otherwise name-classified TUIs get `terminal_grid_size()` (above-bar) and leave a dead band at the bottom.
- **Padding**: `TUI_PAD = 8.0` logical px inset on all four sides in `layout_tui` / `build_tui_text_buffers` / `tui_grid_size`. Don't let cells touch the window edge.
- **Input bar hidden**: `bar_hidden = tui_active` in `Renderer::render`. Keystrokes are forwarded to the PTY via `key_to_pty_bytes` (arrow keys respect `app_cursor_mode` via `\x1bO[ABCD]` vs `\x1b[[ABCD]`).
- **Scrollback**: mouse wheel over a TUI calls `TermGrid::scroll_display(delta)` which moves alacritty's `display_offset`. `cell_grid()` shifts each read by `-display_offset`, and `cursor_pos()` shifts by `+display_offset` so the live cursor disappears into history instead of following the viewport. After scroll_display, `tui_cells` is re-read immediately (it's otherwise only refreshed on PTY output). Any keypress calls `scroll_to_bottom()` to snap back to the live screen — matches xterm/iTerm.
- **Wheel delta**: on macOS trackpads `PixelDelta` fires many small events per gesture. Divide by `cell_h` (not a hard-coded constant) and accumulate the fractional remainder in `scroll_accum` so continuous gestures don't round to zero.
- **Alt-screen caveat**: alt-screen apps keep `history_size = 0`, so wheel-scroll is a no-op in vim/htop by design. Claude runs primary-screen so it *does* get scrollback.

## Block-Char Geometric Rendering

Cell-based TUIs (especially pixel-art avatars, progress bars, and tool-execution indicators) rely on block / half-block / quadrant / circle glyphs tiling seamlessly. Fonts can't guarantee that at non-integer scale factors, so `block_char_geom(ch)` in `renderer.rs` returns a static slice of `SubRect`s for known chars — `layout_tui` paints them as geometry rects in the cell's fg color and `make_tui_row_runs` filters them out of the text runs to avoid double-draw.

Covered chars:
- Full / half blocks: `█ ▀ ▄ ▌ ▐`
- Quadrants: `▘ ▝ ▖ ▗ ▚ ▞ ▙ ▛ ▜ ▟`
- Horizontal eighth blocks: `▁ ▂ ▃ ▅ ▆ ▇` (and `▄` as half)
- Vertical eighth blocks: `▏ ▎ ▍ ▋ ▊ ▉` (and `▌` as half)
- Circle indicators: `⏺ ● ◐ ◑ ◒ ◓` — painted as rounded rects anchored to a cell-centered **square** of side `min(cell_w, cell_h) * 0.55` so dots stay round regardless of cell aspect ratio. `○` stays as a glyph (no outline primitive).

Also: `rect_h` in `layout_tui` is `(next_row_y - row_y) + 1.0` (gap-to-next-row plus 1 px overlap), matching how `rect_w` uses `+1.0` — prevents sub-pixel black seams between cells at fractional `cell_h`.

## PTY Environment

`PtySession::spawn_sized` sets these env vars on the child shell — keep them matching a known-good terminal profile so capability-sniffing TUIs pick their best glyph set:
- `TERM=xterm-256color`, `COLORTERM=truecolor`
- `TERM_PROGRAM=iTerm.app`, `LC_TERMINAL=iTerm2`, `LC_TERMINAL_VERSION=3.5.0` — spoofed because some TUIs have a hardcoded allow-list of "supported" terminals (tested: `Beyond` and `ghostty` are not on claude-code's list).
- `ZDOTDIR`, `BEYONDER_SESSION_ID`

Debug: set `BEYONDER_PTY_LOG=/tmp/pty.log` to dump all raw PTY bytes (escaped) — useful for diagnosing glyph/escape-sequence mismatches without fighting stderr buffering.

## Theming

All renderer colors route through `Renderer::theme` (`beyonder_config::Theme`). Use the `gc(rgb)` helper in `renderer.rs` to convert `[u8;3]` theme slots to `GlyphColor`. Rect colors use the `[f32;4]` slots (`bg`, `surface`, `surface_alt`, `border`) directly. `Renderer::set_theme(theme)` swaps the palette and clears `glyph_buf_cache` so color-baked buffers rebuild. Switch at runtime with `/theme <name>` or by editing the config file. `BUILTIN_THEMES` lists all names. Selection-highlight alpha tints and the model-pill accent are intentionally left as literals.

## Terminal Feature Map

These features are wired end-to-end through `TermGrid` → `TuiCell`/`TerminalCell` → `Renderer`. When adding a new ANSI feature, touch all three layers or the state gets dropped at block finalization.

- **OSC 8 hyperlinks** — `TuiCell.link: Option<Arc<String>>`, `TerminalCell.link: Option<String>` (backward-compat serde default). Read via `cell.hyperlink().uri()` in `cell_grid()`. Underline rendered as theme.blue 1px rect under linked cells in both `layout_tui` (live TUI) and the stored-output `ShellCommand` branch. Hit-testing: `Renderer::link_rects: Vec<([f32;4], String)>` is rebuilt each frame; `App::handle_click` checks it before other hit-tests and calls `open::that(url)`. Hover-cursor-to-pointer is a TODO.
- **SGR mouse reporting (1000/1002/1003/1006)** — `TermGrid::mouse_report_mode()` reads `TermMode::{MOUSE_REPORT_CLICK, MOUSE_DRAG, MOUSE_MOTION, SGR_MOUSE}`. `Renderer::cell_at_phys()` maps physical pixels → 1-based `(col, row)` inside the padded TUI grid. `App` tracks `mouse_button_down` / `last_mouse_cell` and emits `ESC[<Cb;Cx;Cy(M|m)` sequences to the PTY. Wheel events become `Cb=64/65`; modifier bits add +4/+8/+16. Only SGR (1006) encoding is implemented — legacy 1000/1002 without 1006 is a TODO. Mouse reporting only fires when `renderer.tui_active`.
- **Focus reporting (DECSET 1004)** — `TermGrid::focus_reporting_enabled()` reads `TermMode::FOCUS_IN_OUT`. `WindowEvent::Focused(bool)` in app.rs writes `ESC[I` / `ESC[O` to the PTY when the flag is set and a TUI is active.
- **IME / preedit** — `window.set_ime_allowed(true)` in `App::new`. `WindowEvent::Ime` routes `Preedit` to `App::ime_preedit` + `ime_preedit_cursor` (not injected into `InputEditor`); `Commit` inserts text via the normal path. Preedit run syncs to `Renderer::input_preedit` each frame and is drawn inline at the caret in `theme.sky` with a thin underline. `window.set_ime_cursor_area(rect)` is called with the caret rect so IME candidate windows anchor correctly. Caret-x currently approximates via mono `char_w` — real glyphon hit-test is a follow-up.
- **Underline styles + strikethrough** — `UnderlineStyle` enum (None/Single/Double/Curly/Dotted/Dashed) on both `TuiCell` and `TerminalCell`, plus `strikethrough: bool`. alacritty flags mapped in `cell_grid()`. `draw_underline()` in renderer handles both live TUI and stored-output paths. Curly renders as a 2px solid line (true sine-wave TODO); Dotted/Dashed render as dimmed solid lines (segmented TODO).
- **Grapheme / ZWJ emoji** — `TuiCell.grapheme: String` (was `ch: char`), `TerminalCell.grapheme: String` with `#[serde(alias="character")]` and a custom untagged deserializer accepting legacy `char` rows. `cell_grid()` assembles `cell.c + cell.zerowidth()` into one cluster. All shaping uses `Shaping::Advanced`; `block_char_geom` still works off the leading codepoint. Wide-char spacer (null) cells extend the adjacent WIDE cell's text run.
- **Color emoji** — `Renderer::new` loads Apple Color Emoji (`/System/Library/Fonts/Apple Color Emoji.ttc`) and Noto Color Emoji via `font_system.db_mut().load_font_file(...).ok()`. Atlas uses `ColorMode::Web`. Works where glyphon 0.8's swash path supports COLR/sbix; monochrome fallback is acceptable.
- **Scrollback search** — `Cmd+F` (or `Ctrl+F`) toggles search mode; `/find <pattern>` opens pre-filled. Uses `regex::RegexBuilder::new(...).case_insensitive(true)`. `App::search_match_blocks` + `search_current_match` drive a yellow overlay rect in `layout_blocks` (15% alpha matched, 35% alpha current). `Enter`/`F3` = next, `Shift+Enter`/`Shift+F3` = prev, arrow-up/down also navigate, `Esc` exits. Match-count shown as `find (N/M)` in the input prefix. Sub-string-within-block highlight is a TODO.

## Agent System Prompt

`beyonder-runtime::provider::build_system_prompt(cwd, tools)` is injected as `messages[0]` for **both** the Ollama and OpenAI-compat backends — keep them in sync. The prompt is assembled fresh per agent spawn and embeds:

- **Date/time line** — local time with weekday + offset, plus UTC, plus `$TZ` if set.
- **OS line** — `os_name` (e.g. `macOS 14.5` via `sw_vers`, or PRETTY_NAME from `/etc/os-release`), family, arch, login shell basename, and **internet reachability** (`OnceLock`-cached TCP-443 probe against 1.1.1.1 / 8.8.8.8 / cloudflare.com with 1.2s timeout).
- **Toolchain line** — per-tool `available` / `NOT installed` flags for ~30 tools across Core / Network / System / GPU / Security / Web sections, detected via `have()` (which runs `<tool> --version`).
- **Craft sections** — shell craftsmanship, networking (`ping`/`mtr`/`dig`/`curl -w` timing/`ss`/`lsof`/`tcpdump`/`openssl s_client`), system/observability (`htop`/`btop`/`strace`/`dtrace`/`journalctl`/`log stream`/`launchctl`), GPU (`nvidia-smi` CSV query, `rocm-smi`, `powermetrics`), security/cyber (explicit authorization gate for offensive tools, secret-handling rules), and web fetching (`curl-impersonate` for JA3/JA4 fingerprint bypass of anti-bot walls, headless chromium/playwright/puppeteer, html→text via pandoc/lynx/trafilatura, proxy/Tor syntax).
- **OS-specific rules block** that swaps between macOS (BSD userland, `brew`, `pbcopy`, `open`, `fswatch`, case-insensitive FS) / Linux (GNU userland, `apt`/`dnf`/`pacman`/`apk`, `xclip`/`wl-copy`, `xdg-open`, `inotifywait`) / Windows (PowerShell + WSL notes). The sed-inplace rule adapts: `sed -i '' 's/…/…/'` on BSD, `sed -i 's/…/…/'` on GNU, with a `gsed` hint on macOS only when that binary is present.

Env probing (`provider/env_probe.rs`) runs once per process via `OnceLock`. When adding new tool-awareness, extend `EnvProbe`, the toolchain line in `build_system_prompt`, and the relevant craft section in one pass so the prompt doesn't claim a tool exists without probing it.

## Conventions

- Use the existing `beyonder-core` IDs (`BlockId`, `AgentId`, `SessionId`) — all ULID-backed. Don't invent new ID types.
- Workspace dependencies are declared once in root `Cargo.toml`; reference them as `foo = { workspace = true }` in crate manifests.
- `dev` profile uses `opt-level = 1` (beware: debug builds are slower to compile than vanilla but much faster at runtime — needed for the render loop).
