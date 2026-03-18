use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default)]
    pub terminal: Terminal,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Appearance {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_font_family")]
    pub font_family: String,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default = "default_true")]
    pub blur: bool,
    #[serde(default = "default_padding")]
    pub padding: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Terminal {
    #[serde(default = "default_shell")]
    pub shell: String,
    #[serde(default = "default_scrollback")]
    pub scrollback_lines: usize,
    #[serde(default = "default_cursor_style")]
    pub cursor_style: String,
    #[serde(default = "default_true")]
    pub cursor_blink: bool,
}

fn default_theme() -> String { "catppuccin-mocha".into() }
fn default_font_family() -> String { "JetBrains Mono".into() }
fn default_font_size() -> f32 { 15.0 }
fn default_opacity() -> f32 { 0.95 }
fn default_true() -> bool { true }
fn default_padding() -> u32 { 14 }
fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into())
}
fn default_scrollback() -> usize { 10_000 }
fn default_cursor_style() -> String { "beam".into() }

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            font_family: default_font_family(),
            font_size: default_font_size(),
            opacity: default_opacity(),
            blur: default_true(),
            padding: default_padding(),
        }
    }
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            shell: default_shell(),
            scrollback_lines: default_scrollback(),
            cursor_style: default_cursor_style(),
            cursor_blink: default_true(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            terminal: Terminal::default(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let config_path = Self::config_path();
        if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(contents) => match toml::from_str(&contents) {
                    Ok(config) => return config,
                    Err(e) => log::warn!("Failed to parse config: {e}, using defaults"),
                },
                Err(e) => log::warn!("Failed to read config: {e}, using defaults"),
            }
        }
        Self::default()
    }

    fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("sunnyterm")
            .join("config.toml")
    }
}
