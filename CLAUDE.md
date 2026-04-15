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

Ollama is the sole LLM provider (local + Turbo share one backend). Ensure `ollama serve` is running before spawning agents.

## Workspace Layout

Workspace root builds the `beyonder` binary (`src/main.rs`) which is a thin winit `ApplicationHandler` that owns the tokio runtime and delegates to `beyonder-ui::App`. The crates form a layered dependency graph:

- **beyonder-core** — pure data model. `Block`/`BlockId`/`BlockKind`/`BlockContent`, `AgentId`/`AgentInfo`/`AgentState`, `SessionId`, `CapabilitySet`, `ProvenanceChain`, `TuiCell`. No I/O; everything else depends on it.
- **beyonder-store** — SQLite persistence (`rusqlite`, bundled). `BlockStore`, `SessionStore`, migrations. The `Store` wraps a single `Connection`.
- **beyonder-terminal** — PTY management (`portable-pty`) and terminal emulation (`alacritty_terminal`). `PtySession`, `TermGrid`, `BlockBuilder` turns raw PTY output into shell blocks via OSC-133 shell hooks.
- **beyonder-acp** — Agent Client Protocol: messages, transport, `AcpClient`. Streaming events from agent backends.
- **beyonder-runtime** — `AgentSupervisor` spawns and monitors agents; `CapabilityBroker` gates tool use; `tools::` registry executes tool calls; `provider::` holds the `AgentBackend` trait and `OllamaBackend` implementation. Runtime is where the async turn-drivers live (one tokio task per agent, driven via `mpsc` command channels).
- **beyonder-gpu** — wgpu 24 renderer. `Renderer` owns the device/queue/surface and text atlas (glyphon 0.8). `Viewport` handles scrolling. Per-block renderers live in `block_renderers/` (agent_message, approval, shell_block, etc.).
- **beyonder-ui** — the `App` struct: wires supervisor, store, renderer, input editor, history, mode detector, commands. `App::tick()` (called from `about_to_wait`) drains supervisor/broker events so streaming works even when the window is occluded. `App::handle_window_event` + `App::render` are called from `window_event`.
- **beyonder-config** — `BeyonderConfig` loaded from TOML.

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

## Conventions

- Use the existing `beyonder-core` IDs (`BlockId`, `AgentId`, `SessionId`) — all ULID-backed. Don't invent new ID types.
- Workspace dependencies are declared once in root `Cargo.toml`; reference them as `foo = { workspace = true }` in crate manifests.
- `dev` profile uses `opt-level = 1` (beware: debug builds are slower to compile than vanilla but much faster at runtime — needed for the render loop).
