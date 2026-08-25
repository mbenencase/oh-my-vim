//! Configuration: the YAML keymap, its key notation, and resolved settings.

pub mod config;
pub mod keymap;
pub mod keys;

pub use config::{Config, ConfigError, LineNumbers};
pub use keymap::{KeyMap, Resolve, Resolver};
pub use keys::{Key, KeyCode, parse_sequence};
