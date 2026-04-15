use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level Beyonder configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeyonderConfig {
    pub theme: ThemeConfig,
    pub font: FontConfig,
    pub shell: ShellConfig,
    pub data_dir: PathBuf,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_provider")]
    pub provider: String,
}

fn default_model() -> String { "qwen2.5-coder:7b".to_string() }
fn default_provider() -> String { "ollama".to_string() }

impl Default for BeyonderConfig {
    fn default() -> Self {
        Self {
            theme: ThemeConfig::default(),
            font: FontConfig::default(),
            shell: ShellConfig::default(),
            data_dir: default_data_dir(),
            model: default_model(),
            provider: default_provider(),
        }
    }
}

impl BeyonderConfig {
    pub fn load_or_default() -> Self {
        let path = config_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(s) => toml::from_str(&s).unwrap_or_default(),
                Err(_) => Self::default(),
            }
        } else {
            Self::default()
        }
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("beyonder.db")
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&path, toml_str)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    pub background: [f32; 3],
    pub foreground: [f32; 3],
    pub block_border: [f32; 3],
    pub agent_accent: [f32; 3],
    pub approval_accent: [f32; 3],
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            background: [0.08, 0.08, 0.10],
            foreground: [0.90, 0.90, 0.90],
            block_border: [0.20, 0.20, 0.25],
            agent_accent: [0.30, 0.55, 0.90],
            approval_accent: [0.90, 0.60, 0.15],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontConfig {
    pub family: String,
    pub size: f32,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: "monospace".to_string(),
            size: 14.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    pub program: Option<String>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self { program: None }
    }
}

fn config_path() -> PathBuf {
    dirs_next::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("beyonder")
        .join("config.toml")
}

fn default_data_dir() -> PathBuf {
    dirs_next::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("beyonder")
}
