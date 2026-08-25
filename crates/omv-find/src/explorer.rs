use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// One directory level, directories first then files, each alphabetical —
/// the ordering every file tree uses because it makes scanning predictable.
pub fn read_dir(path: &Path, show_hidden: bool) -> std::io::Result<Vec<DirEntry>> {
    let mut entries: Vec<DirEntry> = std::fs::read_dir(path)?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !show_hidden && name.starts_with('.') {
                return None;
            }
            Some(DirEntry {
                is_dir: e.path().is_dir(),
                name,
                path: e.path(),
            })
        })
        .collect();
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    Ok(entries)
}
