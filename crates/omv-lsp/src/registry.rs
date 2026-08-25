use std::collections::HashMap;

/// How to launch a language server for a given language id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSpec {
    pub command: String,
    pub args: Vec<String>,
}

/// Language id → server. Built-in defaults for now; this is the natural place
/// to read a `servers:` section out of config.yaml later.
#[derive(Debug, Clone)]
pub struct Registry {
    servers: HashMap<String, ServerSpec>,
}

impl Registry {
    pub fn with_defaults() -> Self {
        let mut servers = HashMap::new();
        servers.insert(
            "rust".to_string(),
            ServerSpec {
                command: "rust-analyzer".into(),
                args: vec![],
            },
        );
        servers.insert(
            "json".to_string(),
            ServerSpec {
                command: "vscode-json-language-server".into(),
                args: vec!["--stdio".into()],
            },
        );
        Registry { servers }
    }

    pub fn get(&self, language: &str) -> Option<&ServerSpec> {
        self.servers.get(language)
    }

    pub fn set(&mut self, language: impl Into<String>, spec: ServerSpec) {
        self.servers.insert(language.into(), spec);
    }

    /// Whether the server binary is actually on PATH — lets the UI say
    /// "rust-analyzer not found" instead of silently having no LSP.
    pub fn is_available(&self, language: &str) -> bool {
        let Some(spec) = self.get(language) else {
            return false;
        };
        std::env::var_os("PATH")
            .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(&spec.command).is_file()))
            .unwrap_or(false)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Registry::with_defaults()
    }
}
