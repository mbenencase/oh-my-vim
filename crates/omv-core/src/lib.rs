//! Editor core: buffers, modes, motions, text objects, and the action vocabulary.
//!
//! Deliberately free of terminal, async runtime, and protocol dependencies —
//! everything here is synchronous and unit-testable. The UI layer feeds it
//! [`Action`]s and drains the [`Effect`]s it hands back.

pub mod action;
pub mod buffer;
pub mod editor;
pub mod history;
pub mod mode;
pub mod movement;
pub mod textobject;

pub use action::Action;
pub use buffer::{Buffer, Position};
pub use editor::{Editor, Effect, LspIntent, Picker, Register};
pub use mode::Mode;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_columns_account_for_surrogate_pairs() {
        let mut buf = Buffer::empty();
        buf.rope = ropey::Rope::from_str("a🎉b\n");
        // Three chars, but the emoji is two UTF-16 units, so column 3 → 4 units.
        assert_eq!(buf.utf16_column(Position { line: 0, column: 3 }), 4);
        assert_eq!(buf.from_utf16(0, 4), Position { line: 0, column: 3 });
        // A position landing mid-surrogate rounds down to a whole char.
        assert_eq!(buf.from_utf16(0, 2), Position { line: 0, column: 1 });
    }

    #[test]
    fn trailing_newline_is_not_a_phantom_line() {
        let mut buf = Buffer::empty();
        buf.rope = ropey::Rope::from_str("one\ntwo\n");
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.last_line(), 1);
    }

    #[test]
    fn insert_session_undoes_as_one_transaction() {
        let mut ed = Editor::new();
        ed.dispatch(Action::EnterInsertMode, None);
        for c in "hello".chars() {
            ed.insert_char(c);
        }
        ed.dispatch(Action::EnterNormalMode, None);
        assert_eq!(ed.buffer().rope.to_string(), "hello");
        assert!(ed.buffer_mut().undo());
        assert_eq!(
            ed.buffer().rope.to_string(),
            "",
            "one `u` undoes the whole burst"
        );
    }

    #[test]
    fn delete_inside_parens_leaves_the_delimiters() {
        let mut ed = Editor::new();
        ed.buffer_mut().rope = ropey::Rope::from_str("foo(bar, baz)\n");
        ed.buffer_mut().cursor = 6; // inside the parens
        ed.dispatch(Action::DeleteInsideParen, None);
        assert_eq!(ed.buffer().rope.to_string(), "foo()\n");
        assert_eq!(ed.register.text, "bar, baz");
    }

    #[test]
    fn word_motion_crosses_punctuation_classes() {
        let mut ed = Editor::new();
        ed.buffer_mut().rope = ropey::Rope::from_str("foo.bar baz\n");
        ed.buffer_mut().cursor = 0;
        ed.dispatch(Action::MoveWordForward, None);
        assert_eq!(ed.buffer().cursor, 3, "`w` stops on the dot");
        ed.dispatch(Action::MoveWordForward, None);
        assert_eq!(ed.buffer().cursor, 4);
    }

    #[test]
    fn line_wise_paste_lands_on_its_own_line() {
        let mut ed = Editor::new();
        ed.buffer_mut().rope = ropey::Rope::from_str("one\ntwo\n");
        ed.dispatch(Action::YankLine, None);
        ed.dispatch(Action::PasteAfter, None);
        assert_eq!(ed.buffer().rope.to_string(), "one\none\ntwo\n");
    }

    #[test]
    fn quit_refuses_to_discard_unsaved_work() {
        let mut ed = Editor::new();
        ed.insert_char('x');
        let effects = ed.dispatch(Action::Quit, None);
        assert!(!effects.iter().any(|e| matches!(e, Effect::Quit { .. })));
        assert!(matches!(
            ed.dispatch(Action::ForceQuit, None)[0],
            Effect::Quit { force: true }
        ));
    }

    #[test]
    fn counted_g_jumps_to_a_line_but_bare_g_goes_to_the_end() {
        let mut ed = Editor::new();
        ed.buffer_mut().rope = ropey::Rope::from_str("a\nb\nc\nd\ne\n");
        ed.dispatch(Action::MoveFileEnd, Some(3));
        assert_eq!(ed.buffer().cursor_position().line, 2, "3G is line 3");
        ed.dispatch(Action::MoveFileEnd, None);
        assert_eq!(
            ed.buffer().cursor_position().line,
            4,
            "bare G is the last line"
        );
    }

    #[test]
    fn a_count_repeats_a_motion() {
        let mut ed = Editor::new();
        ed.buffer_mut().rope = ropey::Rope::from_str("one two three four\n");
        ed.dispatch(Action::MoveWordForward, Some(3));
        assert_eq!(ed.buffer().cursor, 14, "3w lands on `four`");
    }
}
