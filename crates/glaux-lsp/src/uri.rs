//! `file:` URIs for the paths a server is given.

use std::path::{Path, PathBuf};

/// The `file:` URI of an absolute path.
///
/// Characters outside the unreserved set are percent-encoded byte by byte.
/// A Windows drive path becomes `file:///C:/dir`.
pub fn path_to_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut uri = String::from("file://");
    if !text.starts_with('/') {
        uri.push('/');
    }
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(byte as char);
            }
            b':' if is_drive_colon(&uri) => uri.push(':'),
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

/// `file:///C` followed by a colon is a drive letter, not data.
fn is_drive_colon(uri_so_far: &str) -> bool {
    uri_so_far.len() == "file:///C".len() && uri_so_far.starts_with("file:///")
}

/// The path a `file:` URI names, or `None` for another scheme or bad
/// escapes.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let mut bytes = Vec::with_capacity(rest.len());
    let mut input = rest.bytes();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let high = hex(input.next()?)?;
            let low = hex(input.next()?)?;
            bytes.push(high << 4 | low);
        } else {
            bytes.push(byte);
        }
    }
    let text = String::from_utf8(bytes).ok()?;
    let drive = text.as_bytes();
    let windows = drive.len() >= 3 && drive[0] == b'/' && drive[2] == b':';
    Some(PathBuf::from(if windows { &text[1..] } else { &text }))
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn unix_paths_round_trip_with_escapes() {
        let path = Path::new("/tmp/my cart/game.enemies.luau");
        let uri = path_to_uri(path);
        assert_eq!(uri, "file:///tmp/my%20cart/game.enemies.luau");
        assert_eq!(uri_to_path(&uri).as_deref(), Some(path));
    }

    #[test]
    fn non_ascii_is_encoded_per_utf8_byte() {
        let path = Path::new("/tmp/ñ😀");
        let uri = path_to_uri(path);
        assert_eq!(uri, "file:///tmp/%C3%B1%F0%9F%98%80");
        assert_eq!(uri_to_path(&uri).as_deref(), Some(path));
    }

    #[test]
    fn drive_paths_use_the_windows_form() {
        let uri = path_to_uri(Path::new("C:\\Users\\a b\\x.luau"));
        assert_eq!(uri, "file:///C:/Users/a%20b/x.luau");
        assert_eq!(
            uri_to_path(&uri).as_deref(),
            Some(Path::new("C:/Users/a b/x.luau"))
        );
    }

    #[test]
    fn other_schemes_and_bad_escapes_are_none() {
        assert_eq!(uri_to_path("https://example.com/x"), None);
        assert_eq!(uri_to_path("file:///a%zz"), None);
        assert_eq!(uri_to_path("file:///a%2"), None);
    }
}
