pub const MAX_FILE_SIZE: u64 = 1_048_576;

pub fn is_binary(bytes: &[u8]) -> bool {
    let check_len = bytes.len().min(8192);
    bytes[..check_len].contains(&0)
}

pub fn format_numbered_line(line_num: usize, width: usize, content: &str, marker: bool) -> String {
    if marker {
        format!("> {:>width$} | {}", line_num, content)
    } else {
        format!("  {:>width$} | {}", line_num, content)
    }
}
