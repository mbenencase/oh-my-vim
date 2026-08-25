use std::path::{Path, PathBuf};
use std::str::FromStr;

use lsp_types::Uri;

/// Percent-encode everything a `file://` path segment may not contain literally.
/// `lsp-types` 0.97 wraps `fluent_uri` and offers no path constructor, so this
/// is ours to get right — `/` stays a separator, everything unreserved is kept.
fn encode_segment(segment: &str, out: &mut String) {
    for byte in segment.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
}

/// Absolute path → `file:///...` URI. Relative paths are resolved against cwd,
/// since a language server cannot do anything with a relative one.
pub fn path_to_uri(path: &Path) -> Option<Uri> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    let mut s = String::from("file://");
    for component in absolute.components() {
        if let std::path::Component::RootDir = component {
            continue;
        }
        s.push('/');
        encode_segment(&component.as_os_str().to_string_lossy(), &mut s);
    }
    if s == "file://" {
        s.push('/');
    }
    Uri::from_str(&s).ok()
}

/// `file:///...` URI → path, undoing the percent-encoding.
pub fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let s = uri.as_str();
    let rest = s.strip_prefix("file://")?;
    // Strip an empty authority ("file:///x" → "/x"); a real host isn't a local file.
    let rest = rest
        .strip_prefix('/')
        .map(|r| format!("/{r}"))
        .unwrap_or_else(|| rest.to_string());
    let bytes = rest.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    Some(PathBuf::from(String::from_utf8(out).ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_plain_path() {
        let path = Path::new("/home/u/projects/omv/src/main.rs");
        let uri = path_to_uri(path).unwrap();
        assert_eq!(uri.as_str(), "file:///home/u/projects/omv/src/main.rs");
        assert_eq!(uri_to_path(&uri).unwrap(), path);
    }

    #[test]
    fn round_trips_spaces_and_unicode() {
        let path = Path::new("/tmp/my dir/café.rs");
        let uri = path_to_uri(path).unwrap();
        assert!(
            !uri.as_str().contains(' '),
            "spaces must be encoded: {}",
            uri.as_str()
        );
        assert_eq!(uri_to_path(&uri).unwrap(), path);
    }
}
