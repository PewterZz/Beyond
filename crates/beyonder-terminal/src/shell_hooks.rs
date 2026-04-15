//! Shell hooks for detecting command boundaries.
//! Injects precmd/preexec hooks via shell initialization scripts.
//! Beyonder uses these to know where one command ends and another begins.

/// Zsh hook script injected via ZDOTDIR or sourced at startup.
/// Uses precmd (after command) and preexec (before command) hooks.
/// Signals Beyonder via OSC escape sequences in the PTY stream.
pub fn zsh_init_script(session_id: &str) -> String {
    format!(
        r#"
# Beyonder shell integration for zsh
export BEYONDER_SESSION_ID="{session_id}"

# Suppress the % partial-line marker — Beyonder handles output directly.
unsetopt PROMPT_SP
unsetopt PROMPT_CR

# Signal: command is about to start (preexec = right before command runs)
beyonder_preexec() {{
    local cmd="$1"
    printf '\033]633;A\007'
    printf '\033]633;E;%s\007' "$cmd"
}}

# Signal: command just finished (precmd = right before prompt renders)
beyonder_precmd() {{
    local code=$?
    printf '\033]633;B;%d\007' "$code"
    printf '\033]633;P;Cwd=%s\007' "$PWD"
}}

autoload -Uz add-zsh-hook
add-zsh-hook preexec beyonder_preexec
add-zsh-hook precmd beyonder_precmd
"#
    )
}

/// Bash hook script (less reliable than zsh — zsh is preferred).
pub fn bash_init_script(session_id: &str) -> String {
    format!(
        r#"
# Beyonder shell integration for bash
export BEYONDER_SESSION_ID="{session_id}"

beyonder_preexec() {{
    printf '\033]633;A\007'
    printf '\033]633;E;%s\007' "$BASH_COMMAND"
}}

beyonder_precmd() {{
    local code=$?
    printf '\033]633;B;%d\007' "$code"
    printf '\033]633;P;Cwd=%s\007' "$PWD"
}}

trap 'beyonder_preexec "$BASH_COMMAND"' DEBUG
PROMPT_COMMAND="beyonder_precmd;$PROMPT_COMMAND"
"#
    )
}

/// OSC sequence markers emitted by the shell hooks.
pub mod markers {
    /// Command about to execute.
    pub const CMD_START: &[u8] = b"\x1b]633;A\x07";
    /// Command text follows (terminated by BEL).
    pub const CMD_TEXT_PREFIX: &[u8] = b"\x1b]633;E;";
    /// Command finished (exit code follows).
    pub const CMD_END_PREFIX: &[u8] = b"\x1b]633;B;";
    /// Prompt is about to render.
    pub const PROMPT_START: &[u8] = b"\x1b]633;P\x07";
    pub const BEL: u8 = 0x07;
}
