use std::path::{Path, PathBuf};

use grep_regex::RegexMatcher;
use grep_searcher::sinks::UTF8;
use grep_searcher::{BinaryDetection, SearcherBuilder};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepHit {
    pub path: PathBuf,
    /// 1-based, as every other tool reports it.
    pub line_number: u64,
    pub line: String,
}

/// Search `root` for `pattern`, honouring `.gitignore`.
///
/// The bool is true when `limit` cut the results short. Binary files are skipped.
pub fn search(
    root: &Path,
    pattern: &str,
    limit: usize,
) -> Result<(Vec<GrepHit>, bool), anyhow::Error> {
    let matcher = RegexMatcher::new_line_matcher(pattern)?;
    let mut searcher = SearcherBuilder::new()
        .binary_detection(BinaryDetection::quit(0))
        .line_number(true)
        .build();

    let mut hits = Vec::new();
    let mut truncated = false;

    for entry in ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .build()
    {
        if hits.len() >= limit {
            truncated = true;
            break;
        }
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path().to_path_buf();
        // A single unreadable file shouldn't abort a whole project search.
        let _ = searcher.search_path(
            &matcher,
            &path,
            UTF8(|line_number, line| {
                hits.push(GrepHit {
                    path: path.clone(),
                    line_number,
                    line: line.trim_end().to_string(),
                });
                Ok(hits.len() < limit)
            }),
        );
    }
    Ok((hits, truncated))
}
