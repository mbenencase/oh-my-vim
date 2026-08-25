use serde::Deserialize;

/// Every binding target the YAML keymap may name.
///
/// The macro keeps three things in lockstep: the variant, the name serde accepts
/// from config, and the one-line description. `omv --list-actions` prints the
/// table, and an unknown name in a keymap fails at load time with a suggestion
/// rather than silently doing nothing at 2am.
macro_rules! actions {
    ($( $variant:ident => $name:literal, $doc:literal );* $(;)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
        pub enum Action {
            $(
                #[doc = $doc]
                #[serde(rename = $name)]
                $variant,
            )*
        }

        impl Action {
            /// (action, config name, description) for every action, in declaration order.
            pub const ALL: &'static [(Action, &'static str, &'static str)] = &[
                $( (Action::$variant, $name, $doc), )*
            ];

            pub fn name(self) -> &'static str {
                Self::ALL.iter().find(|(a, _, _)| *a == self).map(|(_, n, _)| *n).unwrap_or("?")
            }

            pub fn describe(self) -> &'static str {
                Self::ALL.iter().find(|(a, _, _)| *a == self).map(|(_, _, d)| *d).unwrap_or("")
            }

            pub fn from_name(name: &str) -> Option<Action> {
                Self::ALL.iter().find(|(_, n, _)| *n == name).map(|(a, _, _)| *a)
            }
        }
    };
}

actions! {
    // ---- motion -------------------------------------------------------------
    MoveLeft            => "move_left",             "Cursor one character left";
    MoveRight           => "move_right",            "Cursor one character right";
    MoveUp              => "move_up",               "Cursor one line up, keeping goal column";
    MoveDown            => "move_down",             "Cursor one line down, keeping goal column";
    MoveWordForward     => "move_word_forward",     "Start of next word";
    MoveWordBackward    => "move_word_backward",    "Start of previous word";
    MoveWordEnd         => "move_word_end",         "End of current or next word";
    MoveLineStart       => "move_line_start",       "First column of the line";
    MoveLineFirstNonBlank => "move_line_first_non_blank", "First non-whitespace character of the line";
    MoveLineEnd         => "move_line_end",         "Last character of the line";
    MoveFileStart       => "move_file_start",       "First line of the buffer";
    MoveFileEnd         => "move_file_end",         "Last line of the buffer";
    MoveHalfPageDown    => "move_half_page_down",   "Half a screen down";
    MoveHalfPageUp      => "move_half_page_up",     "Half a screen up";
    MoveMatchingPair    => "move_matching_pair",    "Jump to the matching bracket";

    // ---- mode switching -----------------------------------------------------
    EnterNormalMode     => "normal_mode",           "Return to normal mode";
    EnterInsertMode     => "insert_mode",           "Insert before the cursor";
    EnterInsertAfter    => "insert_after",          "Insert after the cursor";
    EnterInsertLineStart => "insert_line_start",    "Insert at first non-blank of the line";
    EnterInsertLineEnd  => "insert_line_end",       "Insert at end of the line";
    EnterVisualMode     => "visual_mode",           "Character-wise selection";
    EnterVisualLineMode => "visual_line_mode",      "Line-wise selection";
    EnterCommandMode    => "command_mode",          "Open the `:` prompt";

    // ---- editing ------------------------------------------------------------
    OpenLineBelow       => "open_line_below",       "Open a line below and insert";
    OpenLineAbove       => "open_line_above",       "Open a line above and insert";
    DeleteCharForward   => "delete_char_forward",   "Delete the character under the cursor";
    DeleteCharBackward  => "delete_char_backward",  "Delete the character before the cursor";
    DeleteLine          => "delete_line",           "Delete the whole line into the register";
    DeleteToLineEnd     => "delete_to_line_end",    "Delete from cursor to end of line";
    DeleteWord          => "delete_word",           "Delete to the start of the next word";
    DeleteSelection     => "delete_selection",      "Delete the visual selection";
    ChangeLine          => "change_line",           "Replace the line contents and insert";
    ChangeToLineEnd     => "change_to_line_end",    "Delete to end of line and insert";
    ChangeWord          => "change_word",           "Delete a word and insert";
    ChangeSelection     => "change_selection",      "Replace the visual selection and insert";
    YankLine            => "yank_line",             "Copy the line into the register";
    YankSelection       => "yank_selection",        "Copy the visual selection";
    PasteAfter          => "paste_after",           "Paste after the cursor / below the line";
    PasteBefore         => "paste_before",          "Paste before the cursor / above the line";
    JoinLines           => "join_lines",            "Join the next line onto this one";
    Indent              => "indent",                "Shift line or selection right";
    Outdent             => "outdent",               "Shift line or selection left";
    Undo                => "undo",                  "Undo the last transaction";
    Redo                => "redo",                  "Redo the last undone transaction";

    // ---- text objects -------------------------------------------------------
    DeleteInsideWord    => "delete_inside_word",    "Delete the word under the cursor";
    DeleteAroundWord    => "delete_around_word",    "Delete the word and trailing whitespace";
    DeleteInsideParen   => "delete_inside_paren",   "Delete inside ( )";
    DeleteAroundParen   => "delete_around_paren",   "Delete ( ) and contents";
    DeleteInsideBracket => "delete_inside_bracket", "Delete inside [ ]";
    DeleteAroundBracket => "delete_around_bracket", "Delete [ ] and contents";
    DeleteInsideBrace   => "delete_inside_brace",   "Delete inside { }";
    DeleteAroundBrace   => "delete_around_brace",   "Delete { } and contents";
    DeleteInsideQuote   => "delete_inside_quote",   "Delete inside \" \"";
    DeleteAroundQuote   => "delete_around_quote",   "Delete \" \" and contents";
    ChangeInsideWord    => "change_inside_word",    "Replace the word under the cursor";
    ChangeInsideParen   => "change_inside_paren",   "Replace inside ( )";
    ChangeInsideBracket => "change_inside_bracket", "Replace inside [ ]";
    ChangeInsideBrace   => "change_inside_brace",   "Replace inside { }";
    ChangeInsideQuote   => "change_inside_quote",   "Replace inside \" \"";
    YankInsideWord      => "yank_inside_word",      "Copy the word under the cursor";
    YankInsideParen     => "yank_inside_paren",     "Copy inside ( )";
    YankInsideBrace     => "yank_inside_brace",     "Copy inside { }";

    // ---- panels -------------------------------------------------------------
    ToggleExplorer      => "toggle_explorer",       "Show/hide the file explorer";
    FocusExplorer       => "focus_explorer",        "Move focus to the file explorer";
    FindFiles           => "find_files",            "Fuzzy file picker";
    FindText            => "find_text",             "Project-wide text search";
    FindBuffers         => "find_buffers",          "Switch between open buffers";
    SearchForward       => "search_forward",        "In-buffer search prompt";
    SearchNext          => "search_next",           "Next search match";
    SearchPrev          => "search_prev",           "Previous search match";
    ToggleDiagnostics   => "toggle_diagnostics",    "Show/hide the diagnostics panel";

    // ---- lsp ----------------------------------------------------------------
    LspHover            => "lsp_hover",             "Show hover documentation";
    LspGotoDefinition   => "lsp_goto_definition",   "Jump to definition";
    LspReferences       => "lsp_references",        "List references";
    LspRename           => "lsp_rename",            "Rename the symbol under the cursor";
    LspFormat           => "lsp_format",            "Format the buffer";
    LspNextDiagnostic   => "lsp_next_diagnostic",   "Jump to the next diagnostic";
    LspPrevDiagnostic   => "lsp_prev_diagnostic",   "Jump to the previous diagnostic";

    // ---- buffers & files ----------------------------------------------------
    Save                => "save",                  "Write the buffer to disk";
    Quit                => "quit",                  "Close the buffer, refusing if modified";
    ForceQuit           => "force_quit",            "Close the buffer, discarding changes";
    SaveAndQuit         => "save_and_quit",         "Write, then close";
    NextBuffer          => "next_buffer",           "Cycle to the next buffer";
    PrevBuffer          => "prev_buffer",           "Cycle to the previous buffer";

    // ---- meta ---------------------------------------------------------------
    Nop                 => "nop",                   "Do nothing (useful to mask a default binding)";
}
