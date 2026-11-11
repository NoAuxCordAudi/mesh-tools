//! Internal URI helpers shared by buffers and images: percent-encoding, file-name and relative
//! URI checks, and base64 `data:` URIs.

/// Percent-encode every byte except the RFC 3986 unreserved set.
pub(crate) fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Decode `%XX` escapes; `None` for a malformed escape.
fn percent_decode(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    Some(out)
}

/// A file name written next to the `.gltf`: one path segment, portable characters only.
pub(crate) fn check_file_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("file name is empty".into());
    }
    if name == "." || name == ".." {
        return Err(format!("file name {name:?} is a relative path component"));
    }
    if name.len() > 255 {
        return Err("file name is longer than 255 bytes".into());
    }
    if let Some(c) = name.chars().find(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')) {
        return Err(format!("file name {name:?} contains {c:?} (path separators, drive letters and special characters are not allowed)"));
    }
    Ok(())
}

pub(crate) fn is_data_uri(uri: &str) -> bool {
    uri.get(..5).map_or(false, |p| p.eq_ignore_ascii_case("data:"))
}

/// A non-`data:` buffer URI must be relative and stay inside the `.gltf` directory.
pub(crate) fn check_relative_uri(uri: &str) -> Result<(), String> {
    if uri.is_empty() {
        return Err("uri is empty".into());
    }
    if uri.contains('\\') || uri.starts_with('/') {
        return Err(format!("uri {uri:?} is absolute or uses a backslash"));
    }
    let first = uri.split('/').next().unwrap_or("");
    if first.contains(':') {
        return Err(format!("uri {uri:?} has a scheme; only relative and data: URIs are supported"));
    }
    for seg in uri.split('/') {
        match percent_decode(seg) {
            None => return Err(format!("uri {uri:?} has a malformed percent escape")),
            Some(d) if d == b".." => return Err(format!("uri {uri:?} escapes the model directory with \"..\"")),
            Some(d) if d.iter().any(|&c| matches!(c, b'/' | b'\\' | 0)) => {
                return Err(format!("uri {uri:?} has an escaped path separator or NUL"))
            }
            Some(_) => {}
        }
    }
    Ok(())
}

pub(crate) fn encode_data_uri(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", base64::encode(bytes))
}

/// Split a base64 `data:` URI into its media type and decoded bytes.
pub(crate) fn decode_data_uri(uri: &str) -> Result<(String, Vec<u8>), String> {
    if !is_data_uri(uri) {
        return Err("not a data: URI".into());
    }
    let (header, payload) = uri[5..].split_once(',').ok_or("missing ',' after the media type")?;
    let mime = header.strip_suffix(";base64").ok_or("only base64-encoded data: URIs are supported")?;
    let bytes = base64::decode(payload).map_err(|e| format!("invalid base64: {e}"))?;
    Ok((mime.to_ascii_lowercase(), bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc4648_vectors() {
        for (raw, enc) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
            assert_eq!(encode_data_uri("x/y", raw.as_bytes()), format!("data:x/y;base64,{enc}"));
            assert_eq!(decode_data_uri(&format!("data:x/y;base64,{enc}")).unwrap().1, raw.as_bytes());
        }
    }

    #[test]
    fn percent_round_trip() {
        assert_eq!(percent_encode("a b#é"), "a%20b%23%C3%A9");
        assert_eq!(percent_decode("a%20b%23%C3%A9").unwrap(), "a b#é".as_bytes());
        assert!(percent_decode("%2").is_none());
        assert!(percent_decode("%zz").is_none());
    }
}
