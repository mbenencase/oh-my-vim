//! tree-sitter based syntax highlighting.
//!
//! Produces flat, non-overlapping [`Span`]s in byte offsets, which is what a
//! line-oriented renderer wants. Grammar-specific capture names are folded into
//! a small [`HighlightKind`] set so themes stay language-agnostic.

use std::collections::HashMap;
use std::path::Path;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Parser, Query, QueryCursor, Tree};

/// Semantic categories a theme assigns colours to.
///
/// Richer than what any single grammar's bundled `highlights.scm` emits — e.g.
/// tree-sitter-rust routes integer literals to `@constant`, not `@number`. The
/// extra kinds are here for grammars that do distinguish them, and for the
/// hand-written queries that will eventually replace the bundled ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighlightKind {
    Keyword,
    Function,
    Type,
    Constructor,
    Variable,
    Property,
    Parameter,
    String,
    Number,
    Boolean,
    Comment,
    Operator,
    Punctuation,
    Attribute,
    Constant,
}

impl HighlightKind {
    /// Map a tree-sitter capture name onto a kind, longest-prefix style.
    /// Unknown captures return `None` and simply render unstyled.
    fn from_capture(name: &str) -> Option<HighlightKind> {
        use HighlightKind::*;
        let kind = match name.split('.').next().unwrap_or(name) {
            "keyword" => Keyword,
            "function" | "method" => Function,
            "type" => Type,
            "constructor" => Constructor,
            "variable" => {
                if name.starts_with("variable.parameter") {
                    Parameter
                } else if name.starts_with("variable.builtin") {
                    Constant
                } else {
                    Variable
                }
            }
            "property" | "field" => Property,
            "parameter" => Parameter,
            "string" | "character" | "escape" => String,
            "number" | "float" | "integer" => Number,
            "boolean" => Boolean,
            "comment" => Comment,
            "operator" => Operator,
            "punctuation" | "delimiter" | "bracket" => Punctuation,
            "attribute" | "annotation" => Attribute,
            "constant" | "label" => Constant,
            _ => return None,
        };
        Some(kind)
    }
}

/// A styled byte range. Spans never overlap and are sorted by `start`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub kind: HighlightKind,
}

struct LanguageDef {
    language: Language,
    query: Query,
}

pub struct Highlighter {
    languages: HashMap<&'static str, LanguageDef>,
    parser: Parser,
}

impl Highlighter {
    pub fn new() -> Self {
        let mut languages = HashMap::new();
        for (name, language, query_src) in [
            (
                "rust",
                Language::from(tree_sitter_rust::LANGUAGE),
                tree_sitter_rust::HIGHLIGHTS_QUERY,
            ),
            (
                "json",
                Language::from(tree_sitter_json::LANGUAGE),
                tree_sitter_json::HIGHLIGHTS_QUERY,
            ),
        ] {
            // A grammar whose query fails to compile is skipped rather than fatal:
            // losing colour in one language shouldn't stop the editor from opening.
            match Query::new(&language, query_src) {
                Ok(query) => {
                    languages.insert(name, LanguageDef { language, query });
                }
                Err(e) => tracing_warn(name, &e.to_string()),
            }
        }
        Highlighter {
            languages,
            parser: Parser::new(),
        }
    }

    /// Language id for a path, by extension. Returns `None` for unsupported files.
    pub fn language_for(path: Option<&Path>) -> Option<&'static str> {
        match path?.extension()?.to_str()? {
            "rs" => Some("rust"),
            "json" => Some("json"),
            _ => None,
        }
    }

    pub fn supports(&self, language: &str) -> bool {
        self.languages.contains_key(language)
    }

    pub fn parse(&mut self, language: &str, text: &str) -> Option<Tree> {
        let def = self.languages.get(language)?;
        self.parser.set_language(&def.language).ok()?;
        self.parser.parse(text, None)
    }

    /// Highlight spans for `text`, sorted and de-overlapped.
    ///
    /// Currently reparses the whole document. The rope makes incremental
    /// reparsing available later via `Tree::edit` + `InputEdit`; nothing outside
    /// this function needs to change when that lands.
    pub fn highlight(&mut self, language: &str, text: &str) -> Vec<Span> {
        let Some(tree) = self.parse(language, text) else {
            return Vec::new();
        };
        let Some(def) = self.languages.get(language) else {
            return Vec::new();
        };

        let mut spans: Vec<Span> = Vec::new();
        let mut cursor = QueryCursor::new();
        let names = def.query.capture_names();
        let bytes = text.as_bytes();

        let mut matches = cursor.matches(&def.query, tree.root_node(), bytes);
        while let Some(m) = matches.next() {
            for cap in m.captures {
                let Some(kind) = HighlightKind::from_capture(names[cap.index as usize]) else {
                    continue;
                };
                let range = cap.node.byte_range();
                spans.push(Span {
                    start: range.start,
                    end: range.end,
                    kind,
                });
            }
        }

        // Later, more specific captures win over earlier broad ones, matching how
        // tree-sitter's own highlighter resolves conflicts.
        spans.sort_by_key(|s| (s.start, std::cmp::Reverse(s.end)));
        let mut flat: Vec<Span> = Vec::with_capacity(spans.len());
        for span in spans {
            match flat.last_mut() {
                Some(prev) if span.start < prev.end => {
                    if span.end <= prev.end {
                        continue; // fully nested in a span we already took
                    }
                    let trimmed = Span {
                        start: prev.end,
                        ..span
                    };
                    if trimmed.start < trimmed.end {
                        flat.push(trimmed);
                    }
                }
                _ => flat.push(span),
            }
        }
        flat
    }
}

impl Default for Highlighter {
    fn default() -> Self {
        Highlighter::new()
    }
}

fn tracing_warn(language: &str, message: &str) {
    eprintln!("omv: highlight query for `{language}` failed to compile: {message}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_keywords_are_highlighted() {
        let mut hl = Highlighter::new();
        let spans = hl.highlight("rust", "fn main() { let x = 1; }");
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Keyword));
        // Rust's bundled query has no `@number`; `1` arrives as `@constant`.
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Constant));
    }

    #[test]
    fn spans_never_overlap() {
        let mut hl = Highlighter::new();
        let spans = hl.highlight(
            "rust",
            "struct S { field: Vec<String> }\nimpl S { fn f(&self) {} }",
        );
        for pair in spans.windows(2) {
            assert!(
                pair[0].end <= pair[1].start,
                "overlap: {:?} then {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn extension_maps_to_language() {
        assert_eq!(
            Highlighter::language_for(Some(Path::new("a/b.rs"))),
            Some("rust")
        );
        assert_eq!(Highlighter::language_for(Some(Path::new("a/b.zzz"))), None);
    }
}
