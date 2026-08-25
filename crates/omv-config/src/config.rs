use std::collections::HashMap;
use std::path::{Path, PathBuf};

use omv_core::{Action, Mode};
use serde::Deserialize;

use crate::keymap::{KeyMap, KeyMapError};
use crate::keys::{Key, KeyCode, parse_sequence};

const DEFAULT_YAML: &str = include_str!("../assets/default.yaml");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LineNumbers {
    #[default]
    Absolute,
    Relative,
    None,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawConfig {
    #[serde(default)]
    pub leader: Option<String>,
    #[serde(default)]
    pub indent_width: Option<usize>,
    #[serde(default)]
    pub line_numbers: Option<LineNumbers>,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub keys: HashMap<Mode, HashMap<String, Action>>,
}

/// Fully resolved settings: defaults with the user's file merged over them.
pub struct Config {
    pub leader: Key,
    pub indent_width: usize,
    pub line_numbers: LineNumbers,
    pub theme: String,
    pub keymap: KeyMap,
    /// Kept so `omv --list-actions`/`:map` can show where a binding came from.
    pub bindings: HashMap<Mode, HashMap<String, Action>>,
    pub loaded_from: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("reading {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_yaml_ng::Error,
    },
    #[error("the built-in default keymap is invalid: {0}")]
    Default(String),
    #[error(transparent)]
    KeyMap(#[from] KeyMapError),
    #[error("`leader` must be a single key, got `{0}`")]
    BadLeader(String),
}

impl Config {
    /// `$XDG_CONFIG_HOME/omv/config.yaml`, or `~/.config/omv/config.yaml`.
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("omv").join("config.yaml"))
    }

    pub fn load(path: Option<&Path>) -> Result<Self, ConfigError> {
        let base: RawConfig = serde_yaml_ng::from_str(DEFAULT_YAML)
            .map_err(|e| ConfigError::Default(e.to_string()))?;

        let path = path.map(Path::to_path_buf).or_else(Config::default_path);
        let user = match &path {
            Some(p) if p.exists() => {
                let text = std::fs::read_to_string(p).map_err(|source| ConfigError::Io {
                    path: p.clone(),
                    source,
                })?;
                Some(
                    serde_yaml_ng::from_str::<RawConfig>(&text).map_err(|source| {
                        ConfigError::Parse {
                            path: p.clone(),
                            source,
                        }
                    })?,
                )
            }
            _ => None,
        };

        Config::merge(base, user, path)
    }

    /// Load only the built-in defaults, ignoring any user file.
    pub fn builtin() -> Result<Self, ConfigError> {
        let base: RawConfig = serde_yaml_ng::from_str(DEFAULT_YAML)
            .map_err(|e| ConfigError::Default(e.to_string()))?;
        Config::merge(base, None, None)
    }

    fn merge(
        base: RawConfig,
        user: Option<RawConfig>,
        path: Option<PathBuf>,
    ) -> Result<Self, ConfigError> {
        let mut bindings = base.keys;
        let mut leader_spec = base.leader.unwrap_or_else(|| "<Space>".into());
        let mut indent_width = base.indent_width.unwrap_or(4);
        let mut line_numbers = base.line_numbers.unwrap_or_default();
        let mut theme = base.theme.unwrap_or_else(|| "default".into());
        let mut loaded_from = None;

        if let Some(user) = user {
            if let Some(l) = user.leader {
                leader_spec = l;
            }
            if let Some(w) = user.indent_width {
                indent_width = w;
            }
            if let Some(n) = user.line_numbers {
                line_numbers = n;
            }
            if let Some(t) = user.theme {
                theme = t;
            }
            // Per-key merge, not per-mode replace: overriding `j` shouldn't cost
            // you every other normal-mode binding.
            for (mode, map) in user.keys {
                bindings.entry(mode).or_default().extend(map);
            }
            loaded_from = path;
        }

        // `nop` is how a user deletes a default binding; drop those before building.
        for map in bindings.values_mut() {
            map.retain(|_, action| *action != Action::Nop);
        }

        let leader = parse_sequence(&leader_spec)
            .ok()
            .filter(|s| s.len() == 1)
            .map(|s| s[0])
            .filter(|k| k.code != KeyCode::Leader)
            .ok_or_else(|| ConfigError::BadLeader(leader_spec.clone()))?;

        let keymap = KeyMap::build(&bindings, leader)?;

        Ok(Config {
            leader,
            indent_width,
            line_numbers,
            theme,
            keymap,
            bindings,
            loaded_from,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_keymap_is_valid() {
        let cfg = Config::builtin().expect("default.yaml must parse and build");
        assert_eq!(cfg.leader, Key::char(' '));
        assert!(!cfg.keymap.describe(Mode::Normal).is_empty());
    }

    #[test]
    fn user_bindings_merge_per_key() {
        let base: RawConfig = serde_yaml_ng::from_str(DEFAULT_YAML).unwrap();
        let user: RawConfig =
            serde_yaml_ng::from_str("keys:\n  normal:\n    j: move_up\n").unwrap();
        let cfg = Config::merge(base, Some(user), None).unwrap();
        let normal = &cfg.bindings[&Mode::Normal];
        assert_eq!(normal["j"], Action::MoveUp);
        assert_eq!(
            normal["k"],
            Action::MoveUp,
            "unrelated defaults survive the merge"
        );
    }

    #[test]
    fn nop_removes_a_default_binding() {
        let base: RawConfig = serde_yaml_ng::from_str(DEFAULT_YAML).unwrap();
        let user: RawConfig = serde_yaml_ng::from_str("keys:\n  normal:\n    x: nop\n").unwrap();
        let cfg = Config::merge(base, Some(user), None).unwrap();
        assert!(!cfg.bindings[&Mode::Normal].contains_key("x"));
    }

    #[test]
    fn unknown_action_fails_at_load() {
        let err = serde_yaml_ng::from_str::<RawConfig>("keys:\n  normal:\n    q: teleport\n");
        assert!(
            err.is_err(),
            "a typo in a keymap must not be silently ignored"
        );
    }
}
