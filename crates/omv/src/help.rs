use omv_config::Config;
use omv_core::Mode;

/// One line of the key reference.
pub enum HelpRow {
    /// Vertical breathing room between modes.
    Blank,
    /// A mode heading, e.g. `NORMAL`.
    Mode(&'static str),
    /// A category heading within a mode, e.g. `Motion`.
    Category(&'static str),
    Binding {
        keys: String,
        action: &'static str,
        description: &'static str,
    },
}

/// The `:keys` reference screen: every binding, in the order the action list
/// declares them, grouped by mode and category.
///
/// Built from the *resolved* keymap rather than the YAML, so it shows what is
/// actually bound after the user's config was merged over the defaults —
/// including their own bindings and minus anything they mapped to `nop`.
pub struct Help {
    pub rows: Vec<HelpRow>,
    pub scroll: usize,
    /// Column widths, computed once so the table lines up.
    pub key_width: usize,
    pub action_width: usize,
}

impl Help {
    pub fn build(config: &Config) -> Help {
        let mut rows = Vec::new();
        let mut key_width = 0usize;
        let mut action_width = 0usize;

        for mode in config.keymap.modes() {
            let bindings = config.keymap.describe(mode);
            if bindings.is_empty() {
                continue;
            }
            if !rows.is_empty() {
                rows.push(HelpRow::Blank);
            }
            rows.push(HelpRow::Mode(mode_label(mode)));

            let mut category: Option<&'static str> = None;
            for (keys, action) in bindings {
                if category != Some(action.category()) {
                    category = Some(action.category());
                    rows.push(HelpRow::Category(action.category()));
                }
                key_width = key_width.max(keys.chars().count());
                action_width = action_width.max(action.name().len());
                rows.push(HelpRow::Binding {
                    keys,
                    action: action.name(),
                    description: action.describe(),
                });
            }
        }

        Help {
            rows,
            scroll: 0,
            key_width,
            action_width,
        }
    }

    pub fn scroll_by(&mut self, delta: isize, viewport: usize) {
        let max = self.max_scroll(viewport);
        self.scroll = if delta < 0 {
            self.scroll.saturating_sub((-delta) as usize)
        } else {
            (self.scroll + delta as usize).min(max)
        };
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll = 0;
    }

    pub fn scroll_to_bottom(&mut self, viewport: usize) {
        self.scroll = self.max_scroll(viewport);
    }

    /// Stop scrolling once the last row is on screen, rather than letting the
    /// list run off the top into blank space.
    fn max_scroll(&self, viewport: usize) -> usize {
        self.rows.len().saturating_sub(viewport.max(1))
    }
}

fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "NORMAL",
        Mode::Insert => "INSERT",
        Mode::Visual => "VISUAL",
        Mode::VisualLine => "VISUAL LINE",
        Mode::Command => "COMMAND",
    }
}
