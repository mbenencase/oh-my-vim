use serde::Deserialize;

/// Every binding target the YAML keymap may name.
///
/// The macro keeps three things in lockstep: the variant, the name serde accepts
/// from config, and the one-line description. `omv --list-actions` prints the
/// table, and an unknown name in a keymap fails at load time with a suggestion
/// rather than silently doing nothing at 2am.
macro_rules! actions {
    ($( $variant:ident => $name:literal, $category:literal, $doc:literal );* $(;)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
        pub enum Action {
            $(
                #[doc = $doc]
                #[serde(rename = $name)]
                $variant,
            )*
        }

        impl Action {
            /// (action, config name, category, description), in declaration order.
            /// Declaration order is meaningful: it groups related actions, and
            /// the `:keys` help screen and `--list-actions` both present it as-is.
            pub const ALL: &'static [(Action, &'static str, &'static str, &'static str)] = &[
                $( (Action::$variant, $name, $category, $doc), )*
            ];

            fn entry(self) -> Option<&'static (Action, &'static str, &'static str, &'static str)> {
                Self::ALL.iter().find(|(a, _, _, _)| *a == self)
            }

            pub fn name(self) -> &'static str {
                self.entry().map(|e| e.1).unwrap_or("?")
            }

            /// Heading this action is filed under, e.g. "Motion" or "Text objects".
            pub fn category(self) -> &'static str {
                self.entry().map(|e| e.2).unwrap_or("Other")
            }

            pub fn describe(self) -> &'static str {
                self.entry().map(|e| e.3).unwrap_or("")
            }

            /// Position in [`Action::ALL`], used to sort bindings into a sensible
            /// reading order rather than alphabetically by key.
            pub fn order(self) -> usize {
                Self::ALL.iter().position(|(a, _, _, _)| *a == self).unwrap_or(usize::MAX)
            }

            pub fn from_name(name: &str) -> Option<Action> {
                Self::ALL.iter().find(|(_, n, _, _)| *n == name).map(|(a, _, _, _)| *a)
            }
        }
    };
}

actions! {
    // ---- motion -------------------------------------------------------------
    MoveLeft              => "move_left",                 "Motion",          "Cursor one character left";
    MoveRight             => "move_right",                "Motion",          "Cursor one character right";
    MoveUp                => "move_up",                   "Motion",          "Cursor one line up, keeping goal column";
    MoveDown              => "move_down",                 "Motion",          "Cursor one line down, keeping goal column";
    MoveWordForward       => "move_word_forward",         "Motion",          "Start of next word";
    MoveWordBackward      => "move_word_backward",        "Motion",          "Start of previous word";
    MoveWordEnd           => "move_word_end",             "Motion",          "End of current or next word";
    MoveLineStart         => "move_line_start",           "Motion",          "First column of the line";
    MoveLineFirstNonBlank => "move_line_first_non_blank", "Motion",          "First non-whitespace character of the line";
    MoveLineEnd           => "move_line_end",             "Motion",          "Last character of the line";
    MoveFileStart         => "move_file_start",           "Motion",          "First line of the buffer";
    MoveFileEnd           => "move_file_end",             "Motion",          "Last line of the buffer";
    MoveHalfPageDown      => "move_half_page_down",       "Motion",          "Half a screen down";
    MoveHalfPageUp        => "move_half_page_up",         "Motion",          "Half a screen up";
    MoveMatchingPair      => "move_matching_pair",        "Motion",          "Jump to the matching bracket";

    // ---- mode switching -----------------------------------------------------
    EnterNormalMode       => "normal_mode",               "Modes",           "Return to normal mode";
    EnterInsertMode       => "insert_mode",               "Modes",           "Insert before the cursor";
    EnterInsertAfter      => "insert_after",              "Modes",           "Insert after the cursor";
    EnterInsertLineStart  => "insert_line_start",         "Modes",           "Insert at first non-blank of the line";
    EnterInsertLineEnd    => "insert_line_end",           "Modes",           "Insert at end of the line";
    EnterVisualMode       => "visual_mode",               "Modes",           "Character-wise selection";
    EnterVisualLineMode   => "visual_line_mode",          "Modes",           "Line-wise selection";
    EnterCommandMode      => "command_mode",              "Modes",           "Open the `:` prompt";

    // ---- editing ------------------------------------------------------------
    OpenLineBelow         => "open_line_below",           "Editing",         "Open a line below and insert";
    OpenLineAbove         => "open_line_above",           "Editing",         "Open a line above and insert";
    DeleteCharForward     => "delete_char_forward",       "Editing",         "Delete the character under the cursor";
    DeleteCharBackward    => "delete_char_backward",      "Editing",         "Delete the character before the cursor";
    DeleteLine            => "delete_line",               "Editing",         "Delete the whole line into the register";
    DeleteToLineEnd       => "delete_to_line_end",        "Editing",         "Delete from cursor to end of line";
    DeleteWord            => "delete_word",               "Editing",         "Delete to the start of the next word";
    DeleteSelection       => "delete_selection",          "Editing",         "Delete the visual selection";
    ChangeLine            => "change_line",               "Editing",         "Replace the line contents and insert";
    ChangeToLineEnd       => "change_to_line_end",        "Editing",         "Delete to end of line and insert";
    ChangeWord            => "change_word",               "Editing",         "Delete a word and insert";
    ChangeSelection       => "change_selection",          "Editing",         "Replace the visual selection and insert";
    YankLine              => "yank_line",                 "Editing",         "Copy the line into the register";
    YankSelection         => "yank_selection",            "Editing",         "Copy the visual selection";
    PasteAfter            => "paste_after",               "Editing",         "Paste after the cursor / below the line";
    PasteBefore           => "paste_before",              "Editing",         "Paste before the cursor / above the line";
    JoinLines             => "join_lines",                "Editing",         "Join the next line onto this one";
    Indent                => "indent",                    "Editing",         "Shift line or selection right";
    Outdent               => "outdent",                   "Editing",         "Shift line or selection left";
    Undo                  => "undo",                      "Editing",         "Undo the last transaction";
    Redo                  => "redo",                      "Editing",         "Redo the last undone transaction";

    // ---- text objects -------------------------------------------------------
    DeleteInsideWord      => "delete_inside_word",        "Text objects",    "Delete the word under the cursor";
    DeleteAroundWord      => "delete_around_word",        "Text objects",    "Delete the word and trailing whitespace";
    DeleteInsideParen     => "delete_inside_paren",       "Text objects",    "Delete inside ( )";
    DeleteAroundParen     => "delete_around_paren",       "Text objects",    "Delete ( ) and contents";
    DeleteInsideBracket   => "delete_inside_bracket",     "Text objects",    "Delete inside [ ]";
    DeleteAroundBracket   => "delete_around_bracket",     "Text objects",    "Delete [ ] and contents";
    DeleteInsideBrace     => "delete_inside_brace",       "Text objects",    "Delete inside { }";
    DeleteAroundBrace     => "delete_around_brace",       "Text objects",    "Delete { } and contents";
    DeleteInsideQuote     => "delete_inside_quote",       "Text objects",    "Delete inside \" \"";
    DeleteAroundQuote     => "delete_around_quote",       "Text objects",    "Delete \" \" and contents";
    ChangeInsideWord      => "change_inside_word",        "Text objects",    "Replace the word under the cursor";
    ChangeInsideParen     => "change_inside_paren",       "Text objects",    "Replace inside ( )";
    ChangeInsideBracket   => "change_inside_bracket",     "Text objects",    "Replace inside [ ]";
    ChangeInsideBrace     => "change_inside_brace",       "Text objects",    "Replace inside { }";
    ChangeInsideQuote     => "change_inside_quote",       "Text objects",    "Replace inside \" \"";
    YankInsideWord        => "yank_inside_word",          "Text objects",    "Copy the word under the cursor";
    YankInsideParen       => "yank_inside_paren",         "Text objects",    "Copy inside ( )";
    YankInsideBrace       => "yank_inside_brace",         "Text objects",    "Copy inside { }";

    // ---- panels -------------------------------------------------------------
    ToggleExplorer        => "toggle_explorer",           "Panels",          "Show/hide the file explorer";
    FocusExplorer         => "focus_explorer",            "Panels",          "Move focus to the file explorer";
    FindFiles             => "find_files",                "Panels",          "Fuzzy file picker";
    FindText              => "find_text",                 "Panels",          "Project-wide text search";
    FindBuffers           => "find_buffers",              "Panels",          "Switch between open buffers";
    SearchForward         => "search_forward",            "Panels",          "In-buffer search prompt";
    SearchNext            => "search_next",               "Panels",          "Next search match";
    SearchPrev            => "search_prev",               "Panels",          "Previous search match";
    Substitute            => "substitute",                "Panels",          "Find and replace text in this buffer";
    ToggleDiagnostics     => "toggle_diagnostics",        "Panels",          "Show/hide the diagnostics panel";
    ToggleTerminal        => "toggle_terminal",           "Panels",          "Show/hide the shell docked at the bottom";
    ShowKeys              => "show_keys",                 "Panels",          "Show every key binding and what it does";

    // ---- windows ------------------------------------------------------------
    SplitVertical         => "split_vertical",            "Windows",         "Split the window side by side";
    SplitHorizontal       => "split_horizontal",          "Windows",         "Split the window top and bottom";
    FocusWindowLeft       => "window_left",               "Windows",         "Focus the window to the left";
    FocusWindowDown       => "window_down",               "Windows",         "Focus the window below";
    FocusWindowUp         => "window_up",                 "Windows",         "Focus the window above";
    FocusWindowRight      => "window_right",              "Windows",         "Focus the window to the right";
    CloseWindow           => "window_close",              "Windows",         "Close the focused window";
    OnlyWindow            => "window_only",               "Windows",         "Close every window but this one";

    // ---- lsp ----------------------------------------------------------------
    LspHover              => "lsp_hover",                 "LSP",             "Show hover documentation";
    LspGotoDefinition     => "lsp_goto_definition",       "LSP",             "Jump to definition";
    LspReferences         => "lsp_references",            "LSP",             "List references";
    LspRename             => "lsp_rename",                "LSP",             "Rename the symbol under the cursor";
    LspFormat             => "lsp_format",                "LSP",             "Format the buffer";
    LspNextDiagnostic     => "lsp_next_diagnostic",       "LSP",             "Jump to the next diagnostic";
    LspPrevDiagnostic     => "lsp_prev_diagnostic",       "LSP",             "Jump to the previous diagnostic";

    // ---- buffers & files ----------------------------------------------------
    Save                  => "save",                      "Files & buffers", "Write the buffer to disk";
    Quit                  => "quit",                      "Files & buffers", "Close the buffer, refusing if modified";
    ForceQuit             => "force_quit",                "Files & buffers", "Close the buffer, discarding changes";
    SaveAndQuit           => "save_and_quit",             "Files & buffers", "Write, then close";
    NextBuffer            => "next_buffer",               "Files & buffers", "Cycle to the next buffer";
    PrevBuffer            => "prev_buffer",               "Files & buffers", "Cycle to the previous buffer";

    // ---- meta ---------------------------------------------------------------
    Nop                   => "nop",                       "Meta",            "Do nothing (useful to mask a default binding)";
}
