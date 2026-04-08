use std::path::{Path, PathBuf};

pub const MAX_FILE_SIZE: u64 = 1_048_576;

pub fn validate_path(file_path: &str, project_root: &Path) -> Result<PathBuf, String> {
    let path = Path::new(file_path);
    let canonical = path.canonicalize().or_else(|_| {
        if let Some(parent) = path.parent() {
            let canon_parent = parent.canonicalize().map_err(|e| {
                format!("Path not accessible: {}: {}", file_path, e)
            })?;
            Ok(canon_parent.join(path.file_name().unwrap_or_default()))
        } else {
            Err(format!("Path not accessible: {}", file_path))
        }
    })?;

    if !canonical.starts_with(project_root) {
        return Err(format!(
            "Access denied: '{}' is outside project root '{}'",
            file_path,
            project_root.display()
        ));
    }
    Ok(canonical)
}

pub fn is_binary(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
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
