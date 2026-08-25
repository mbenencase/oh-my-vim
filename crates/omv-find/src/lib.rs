//! File discovery, fuzzy matching, and project-wide text search.
//!
//! Everything here is synchronous and blocking by design — the UI runs it on a
//! worker thread and streams results back over a channel, so a slow filesystem
//! never stalls the render loop.

pub mod explorer;
pub mod grep;
pub mod matcher;

pub use explorer::{DirEntry, read_dir};
pub use grep::{GrepHit, search};
pub use matcher::{Match, Matcher};

use std::path::{Path, PathBuf};

/// Walk `root` for files, honouring `.gitignore` and skipping hidden entries.
///
/// `limit` caps the result set; the returned bool is true when the walk was cut
/// short, so the picker can say "showing first N" instead of implying it found
/// everything.
pub fn walk_files(root: &Path, limit: usize) -> (Vec<PathBuf>, bool) {
    let mut out = Vec::new();
    let mut truncated = false;
    for entry in ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .build()
    {
        let Ok(entry) = entry else { continue };
        if entry.file_type().is_some_and(|t| t.is_file()) {
            if out.len() >= limit {
                truncated = true;
                break;
            }
            out.push(entry.into_path());
        }
    }
    (out, truncated)
}

/// Path relative to `root` when possible — pickers should show `src/main.rs`,
/// not the user's whole home directory.
pub fn display_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}
