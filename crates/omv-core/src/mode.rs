use serde::Deserialize;

/// Editing modes. Panels (explorer, pickers) capture input at the UI layer and
/// never enter here — modes describe the *buffer's* input interpretation only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Normal,
    Insert,
    Visual,
    VisualLine,
    /// The `:` prompt. Text is accumulated in `Editor::command_line`.
    Command,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Visual => "VISUAL",
            Mode::VisualLine => "V-LINE",
            Mode::Command => "COMMAND",
        }
    }

    pub fn is_visual(self) -> bool {
        matches!(self, Mode::Visual | Mode::VisualLine)
    }
}
