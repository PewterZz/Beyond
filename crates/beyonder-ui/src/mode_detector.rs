//! Detects the input mode from the current text in the input editor.
//! Three modes: Shell (default), Agent (@name prefix), Command (/ prefix)

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    /// Goes to the active shell PTY.
    Shell,
    /// Routes to an agent (identified by name after @).
    Agent { name: String },
    /// A Beyonder slash command.
    Command { cmd: String },
}

/// Detect the input mode from the current input string.
pub fn detect_mode(input: &str) -> InputMode {
    let trimmed = input.trim_start();

    if let Some(rest) = trimmed.strip_prefix('/') {
        let cmd = rest.split_whitespace().next().unwrap_or("").to_string();
        return InputMode::Command { cmd };
    }

    if let Some(rest) = trimmed.strip_prefix('@') {
        let name = rest.split_whitespace().next().unwrap_or("claude").to_string();
        return InputMode::Agent { name };
    }

    InputMode::Shell
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_mode() {
        assert_eq!(detect_mode("ls -la"), InputMode::Shell);
        assert_eq!(detect_mode("cargo build"), InputMode::Shell);
    }

    #[test]
    fn test_agent_mode() {
        assert_eq!(
            detect_mode("@claude fix the bug in main.rs"),
            InputMode::Agent { name: "claude".to_string() }
        );
    }

    #[test]
    fn test_command_mode() {
        assert_eq!(
            detect_mode("/agent list"),
            InputMode::Command { cmd: "agent".to_string() }
        );
    }
}
