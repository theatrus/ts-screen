use serde_json;

pub fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len-3])
    }
}

pub fn extract_filename(metadata: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(metadata).ok()?;
    json.get("FileName")
        .and_then(|f| f.as_str())
        .map(|path| {
            // Extract just the filename from the full path
            path.split(&['\\', '/'][..])
                .last()
                .unwrap_or(path)
                .to_string()
        })
}