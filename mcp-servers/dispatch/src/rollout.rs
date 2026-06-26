//! Reading + curating codex rollout logs.
//!
//! codex writes a JSONL "rollout" per session at
//! `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-<ts>-<session-uuid>.jsonl`, appended
//! live while a run is in progress. `dispatch_logs` reads it to show progress —
//! but the raw rollout is mostly noise (the giant system prompt, token counts,
//! encrypted reasoning, duplicate developer messages), so we **curate** it to a
//! compact timeline and slice it by **line range** so a long session can't blow
//! the MCP output budget. The `locate_*` functions positively identify the rollout a
//! task produced — by pre-spawn snapshot diff, by the prompt nonce, or by session id
//! (which also feeds `codex exec resume`) — rather than guessing by cwd alone.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

const RENDER_BYTE_CAP: usize = 40 * 1024;
const DEFAULT_TAIL_LINES: usize = 150;
const SCAN_CAP: usize = 60;
/// How many opening lines of a rollout `locate_by_nonce` scans for the marker — the
/// dispatch prompt is recorded as a `user_message` among the first events.
const NONCE_SCAN_LINES: usize = 64;

/// The curation categories `dispatch_logs(kinds=…)` can select. `reasoning` is
/// excluded by default — codex reasoning is encrypted and renders as a bare marker.
pub fn default_kinds() -> Vec<String> {
    ["lifecycle", "messages", "tools", "edits"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn kind_of(t: &str, pt: &str) -> Option<&'static str> {
    match (t, pt) {
        ("event_msg", "task_started") | ("event_msg", "task_complete") => Some("lifecycle"),
        ("event_msg", "user_message") | ("event_msg", "agent_message") => Some("messages"),
        ("response_item", "custom_tool_call") | ("response_item", "custom_tool_call_output") => {
            Some("tools")
        }
        ("event_msg", "patch_apply_end") => Some("edits"),
        ("response_item", "reasoning") => Some("reasoning"),
        // noise: session_meta, turn_context, event_msg/token_count, response_item/message
        _ => None,
    }
}

pub struct Rendered {
    pub lines: Vec<String>,
    pub total: usize,
}

/// Curate a rollout JSONL string into a compact timeline, keeping only the
/// selected `kinds`. Unparseable lines (e.g. a half-written trailing line while
/// codex is still appending) are skipped.
pub fn curate(jsonl: &str, kinds: &[String]) -> Rendered {
    let mut lines = Vec::new();
    for raw in jsonl.lines() {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let o: Value = match serde_json::from_str(raw) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some((kind, line)) = render(&o)
            && kinds.iter().any(|k| k == kind)
        {
            lines.push(line);
        }
    }
    let total = lines.len();
    Rendered { lines, total }
}

fn render(o: &Value) -> Option<(&'static str, String)> {
    let t = o.get("type")?.as_str()?;
    let p = o.get("payload")?;
    let pt = p.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let kind = kind_of(t, pt)?;
    let line = match (t, pt) {
        ("event_msg", "task_started") => "▶ started".to_string(),
        ("event_msg", "task_complete") => {
            let last = field_str(p, "last_agent_message");
            match p.get("duration_ms").and_then(|v| v.as_u64()) {
                Some(d) => format!("✓ complete ({}s): {}", d / 1000, oneline(last, 200)),
                None => format!("✓ complete: {}", oneline(last, 200)),
            }
        }
        ("event_msg", "user_message") => {
            format!("[user] {}", oneline(field_str(p, "message"), 300))
        }
        ("event_msg", "agent_message") => {
            format!("[codex] {}", oneline(field_str(p, "message"), 500))
        }
        ("response_item", "custom_tool_call") => {
            let name = field_str(p, "name");
            format!("[tool] {}: {}", name, oneline(field_str(p, "input"), 160))
        }
        ("response_item", "custom_tool_call_output") => {
            format!("[result] {}", oneline(field_str(p, "output"), 200))
        }
        ("event_msg", "patch_apply_end") => {
            let ok = p.get("success").and_then(|v| v.as_bool()).unwrap_or(true);
            format!(
                "[edit{}] {}",
                if ok { "" } else { " FAILED" },
                patch_files(p)
            )
        }
        ("response_item", "reasoning") => "[thinking] (reasoning)".to_string(),
        _ => return None,
    };
    Some((kind, line))
}

fn field_str<'a>(p: &'a Value, key: &str) -> &'a str {
    p.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

fn oneline(s: &str, cap: usize) -> String {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > cap {
        let cut: String = one.chars().take(cap).collect();
        format!("{cut}…")
    } else {
        one
    }
}

fn patch_files(p: &Value) -> String {
    if let Some(ch) = p.get("changes").and_then(|v| v.as_object()) {
        let mut parts: Vec<String> = ch
            .iter()
            .map(|(path, meta)| {
                let kind = meta
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("change");
                let base = Path::new(path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(path);
                format!("{kind} {base}")
            })
            .collect();
        if !parts.is_empty() {
            parts.sort();
            return parts.join(", ");
        }
    }
    let stdout = field_str(p, "stdout");
    if stdout.is_empty() {
        "(files changed)".to_string()
    } else {
        oneline(stdout, 160)
    }
}

/// Slice rendered lines to a 1-based inclusive `[start, end]` window, defaulting
/// to the last `DEFAULT_TAIL_LINES`, and cap total bytes. Returns
/// `(text, shown_start, shown_end, byte_capped)`.
pub fn window(
    lines: &[String],
    start: Option<usize>,
    end: Option<usize>,
) -> (String, usize, usize, bool) {
    let total = lines.len();
    if total == 0 {
        return (String::new(), 0, 0, false);
    }
    let (mut s, mut e) = match (start, end) {
        (Some(s), Some(e)) => (s, e),
        (Some(s), None) => (s, total),
        (None, Some(e)) => (1, e),
        (None, None) => (total.saturating_sub(DEFAULT_TAIL_LINES) + 1, total),
    };
    s = s.clamp(1, total);
    e = e.clamp(s, total);
    let mut text = lines[(s - 1)..=(e - 1)].join("\n");
    let mut capped = false;
    if text.len() > RENDER_BYTE_CAP {
        text = text.chars().take(RENDER_BYTE_CAP).collect();
        text.push_str("\n…[truncated at byte cap — narrow the line range]");
        capped = true;
    }
    (text, s, e, capped)
}

pub fn read_to_string(path: &Path) -> std::io::Result<String> {
    std::fs::read_to_string(path)
}

// ── locating a session's rollout file ─────────────────────

fn codex_home() -> PathBuf {
    if let Some(h) = std::env::var_os("CODEX_HOME") {
        return PathBuf::from(h);
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home).join(".codex")
}

/// Snapshot the set of rollout files that exist right now. Taken **before** spawning
/// a codex child so the post-spawn search can require a *new* file and never match a
/// pre-existing session (e.g. an `aside` run) that merely shares the cwd.
pub fn session_snapshot() -> HashSet<PathBuf> {
    snapshot_in(&codex_home().join("sessions"))
}

fn snapshot_in(root: &Path) -> HashSet<PathBuf> {
    let mut files = Vec::new();
    collect(root, &mut files, 0);
    files.into_iter().map(|(p, _)| p).collect()
}

/// Find the rollout THIS run produced: the newest one matching `working_dir` that is
/// NOT in `exclude` (the pre-spawn snapshot) and, when `floor` is set, modified at or
/// after it. The snapshot exclusion is the real discriminator for a fresh run; `floor`
/// alone (with an empty `exclude`) serves the snapshot-less recovery path (e.g. after
/// a server restart). The two are AND-ed — never OR-ed, which would re-admit a
/// pre-existing session whose mtime happens to be recent.
pub fn locate_new_by_cwd(
    working_dir: &Path,
    exclude: &HashSet<PathBuf>,
    floor: Option<SystemTime>,
) -> Option<(PathBuf, String)> {
    locate_new_in(&codex_home().join("sessions"), working_dir, exclude, floor)
}

fn locate_new_in(
    root: &Path,
    working_dir: &Path,
    exclude: &HashSet<PathBuf>,
    floor: Option<SystemTime>,
) -> Option<(PathBuf, String)> {
    let mut files = Vec::new();
    collect(root, &mut files, 0);
    files.sort_by(|a, b| b.1.cmp(&a.1)); // newest first
    let want = working_dir.to_string_lossy().to_string();
    let want_canon = working_dir.canonicalize().ok();
    for (path, mtime) in files.into_iter().take(SCAN_CAP) {
        if exclude.contains(&path) {
            continue;
        }
        if let Some(floor) = floor
            && mtime < floor
        {
            continue;
        }
        if let Some((cwd, sid)) = read_session_meta(&path)
            && cwd_matches(&cwd, &want, want_canon.as_deref())
        {
            return Some((path, sid));
        }
    }
    None
}

/// Locate a rollout by its codex session id. The session id is the trailing UUID of
/// the rollout filename (`rollout-<ts>-<sid>.jsonl`), so this is a cheap, exact
/// filename match with no scan cap — used by `dispatch_steer`'s resume (the session id
/// is already known) and as a deterministic re-locate.
pub fn locate_by_session_id(sid: &str) -> Option<PathBuf> {
    locate_by_session_id_in(&codex_home().join("sessions"), sid)
}

fn locate_by_session_id_in(root: &Path, sid: &str) -> Option<PathBuf> {
    if sid.is_empty() {
        return None;
    }
    let mut files = Vec::new();
    collect(root, &mut files, 0);
    let suffix = format!("-{sid}.jsonl");
    files.into_iter().map(|(p, _)| p).find(|p| {
        p.file_name()
            .and_then(|s| s.to_str())
            .map(|n| n.ends_with(&suffix))
            .unwrap_or(false)
    })
}

/// Find the rollout this task produced by its embedded nonce: the newest rollout
/// matching `working_dir` whose opening events include a `user_message` containing
/// `nonce`. Positive identity — survives a concurrent same-cwd codex (e.g. `aside`)
/// that a snapshot / time gate alone could not distinguish.
pub fn locate_by_nonce(working_dir: &Path, nonce: &str) -> Option<(PathBuf, String)> {
    locate_by_nonce_in(&codex_home().join("sessions"), working_dir, nonce)
}

fn locate_by_nonce_in(root: &Path, working_dir: &Path, nonce: &str) -> Option<(PathBuf, String)> {
    if nonce.is_empty() {
        return None;
    }
    let mut files = Vec::new();
    collect(root, &mut files, 0);
    files.sort_by(|a, b| b.1.cmp(&a.1)); // newest first
    let want = working_dir.to_string_lossy().to_string();
    let want_canon = working_dir.canonicalize().ok();
    for (path, _) in files.into_iter().take(SCAN_CAP) {
        if let Some((cwd, sid)) = read_session_meta(&path)
            && cwd_matches(&cwd, &want, want_canon.as_deref())
            && rollout_has_nonce(&path, nonce)
        {
            return Some((path, sid));
        }
    }
    None
}

fn collect(dir: &Path, out: &mut Vec<(PathBuf, SystemTime)>, depth: usize) {
    if depth > 5 {
        return;
    }
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out, depth + 1);
        } else if let Some(name) = p.file_name().and_then(|s| s.to_str())
            && name.starts_with("rollout-")
            && name.ends_with(".jsonl")
        {
            let mtime = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            out.push((p, mtime));
        }
    }
}

fn read_session_meta(path: &Path) -> Option<(String, String)> {
    use std::io::{BufRead, BufReader};
    let f = std::fs::File::open(path).ok()?;
    let mut first = String::new();
    BufReader::new(f).read_line(&mut first).ok()?;
    let o: Value = serde_json::from_str(first.trim()).ok()?;
    if o.get("type")?.as_str()? != "session_meta" {
        return None;
    }
    let p = o.get("payload")?;
    let cwd = p.get("cwd")?.as_str()?.to_string();
    // session_id is the canonical field; older codex logs carried only `id`.
    let sid = p
        .get("session_id")
        .or_else(|| p.get("id"))
        .and_then(|v| v.as_str())?
        .to_string();
    Some((cwd, sid))
}

/// Whether `cwd` (from a rollout's session_meta) refers to the same directory as
/// `want`, comparing the raw strings first and then canonicalized paths.
fn cwd_matches(cwd: &str, want: &str, want_canon: Option<&Path>) -> bool {
    if cwd == want {
        return true;
    }
    if let Some(wc) = want_canon
        && Path::new(cwd).canonicalize().ok().as_deref() == Some(wc)
    {
        return true;
    }
    false
}

/// Scan the opening lines of a rollout for the dispatch `nonce` inside a
/// `user_message` event. Reads at most `NONCE_SCAN_LINES` lines — the rendered prompt
/// (which carries the marker) is recorded among the first events.
pub fn rollout_has_nonce(path: &Path, nonce: &str) -> bool {
    use std::io::{BufRead, BufReader};
    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    for line in BufReader::new(f)
        .lines()
        .map_while(Result::ok)
        .take(NONCE_SCAN_LINES)
    {
        let o: Value = match serde_json::from_str(line.trim()) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if o.get("type").and_then(|v| v.as_str()) != Some("event_msg") {
            continue;
        }
        let Some(p) = o.get("payload") else { continue };
        if p.get("type").and_then(|v| v.as_str()) != Some("user_message") {
            continue;
        }
        if p.get("message")
            .and_then(|v| v.as_str())
            .map(|m| m.contains(nonce))
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

/// Whether the rollout at `path` was produced by codex session `sid` (its
/// session_meta session_id matches). Used to validate a steered task's inherited,
/// authoritative session id — not circular there because the id was set from the
/// parent, not guessed from this file.
pub fn rollout_has_session_id(path: &Path, sid: &str) -> bool {
    if sid.is_empty() {
        return false;
    }
    read_session_meta(path)
        .map(|(_, s)| s == sid)
        .unwrap_or(false)
}

/// Whether the rollout at `path` is for `working_dir` AND was last modified at or after
/// `floor` (the task's start, minus tolerance). For a row with no nonce and no
/// authoritative session id (a legacy row), mtime is the only independent signal that
/// the rollout is not a stale pre-existing same-cwd session — this is what breaks the
/// circular `session_id == session_id` self-comparison on a poisoned row.
pub fn rollout_cwd_after(path: &Path, working_dir: &Path, floor: Option<SystemTime>) -> bool {
    let Some((cwd, _)) = read_session_meta(path) else {
        return false;
    };
    let want = working_dir.to_string_lossy().to_string();
    if !cwd_matches(&cwd, &want, working_dir.canonicalize().ok().as_deref()) {
        return false;
    }
    match floor {
        Some(f) => file_mtime(path).map(|m| m >= f).unwrap_or(false),
        None => true,
    }
}

fn file_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
{"type":"session_meta","payload":{"session_id":"x","cwd":"/w"}}
{"type":"turn_context","payload":{}}
{"type":"event_msg","payload":{"type":"task_started","started_at":1}}
{"type":"event_msg","payload":{"type":"user_message","message":"do the thing"}}
{"type":"response_item","payload":{"type":"reasoning","encrypted_content":"zzz","summary":[]}}
{"type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","input":"*** Begin Patch\n*** Add File: a.txt"}}
{"type":"event_msg","payload":{"type":"patch_apply_end","success":true,"changes":{"/w/a.txt":{"type":"add"}}}}
{"type":"event_msg","payload":{"type":"agent_message","message":"done"}}
{"type":"event_msg","payload":{"type":"token_count","payload":{}}}
{"type":"event_msg","payload":{"type":"task_complete","last_agent_message":"finished","duration_ms":1500}}
{"type":"response_item","payload":{"type":"message","role":"developer","content":[]}}
{"type":"event_msg","payload":{"type":"agent_me
"#;

    #[test]
    fn curate_keeps_signal_drops_noise_and_tolerates_partial_tail() {
        let r = curate(SAMPLE, &default_kinds());
        // default kinds exclude reasoning; keep: started, user, tool, edit, codex, complete = 6
        assert_eq!(r.total, 6, "lines: {:?}", r.lines);
        assert!(r.lines.iter().any(|l| l.starts_with("[user]")));
        assert!(r.lines.iter().any(|l| l.contains("apply_patch")));
        assert!(
            r.lines
                .iter()
                .any(|l| l.starts_with("[edit]") && l.contains("a.txt"))
        );
        assert!(r.lines.iter().any(|l| l.starts_with("[codex]")));
        assert!(r.lines.iter().any(|l| l.starts_with("✓ complete")));
        // reasoning excluded by default; noise never rendered
        assert!(!r.lines.iter().any(|l| l.starts_with("[thinking]")));
    }

    #[test]
    fn curate_reasoning_opt_in() {
        let kinds: Vec<String> = vec!["reasoning".to_string()];
        let r = curate(SAMPLE, &kinds);
        assert_eq!(r.total, 1);
        assert!(r.lines[0].starts_with("[thinking]"));
    }

    #[test]
    fn window_defaults_to_full_when_small_and_slices_by_range() {
        let lines: Vec<String> = (1..=10).map(|i| format!("line {i}")).collect();
        let (_t, s, e, _) = window(&lines, None, None);
        assert_eq!((s, e), (1, 10));
        let (txt, s2, e2, _) = window(&lines, Some(3), Some(5));
        assert_eq!((s2, e2), (3, 5));
        assert_eq!(txt, "line 3\nline 4\nline 5");
        // out-of-range clamps
        let (_t2, s3, e3, _) = window(&lines, Some(8), Some(99));
        assert_eq!((s3, e3), (8, 10));
    }

    // ── rollout locating ──────────────────────────────────

    use serde_json::json;

    fn test_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "dispatch-rollout-test-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    fn write_rollout(dir: &Path, sid: &str, cwd: &str, user_msg: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(format!("rollout-2026-06-27T00-00-00-{sid}.jsonl"));
        let meta = json!({"type":"session_meta","payload":{"session_id":sid,"cwd":cwd}});
        let um = json!({"type":"event_msg","payload":{"type":"user_message","message":user_msg}});
        std::fs::write(&path, format!("{meta}\n{um}\n")).unwrap();
        path
    }

    #[test]
    fn locate_new_excludes_snapshot_and_finds_fresh_only() {
        let root = test_root("locate-new");
        let day = root.join("2026/06/27");
        let stale = write_rollout(&day, "stale-aaa", "/w", "old prompt");
        // snapshot taken "before spawn" sees only the pre-existing (aside) rollout
        let snapshot: HashSet<PathBuf> = [stale.clone()].into_iter().collect();
        let fresh = write_rollout(&day, "fresh-bbb", "/w", "new prompt");

        let got = locate_new_in(&root, Path::new("/w"), &snapshot, None);
        assert_eq!(got, Some((fresh.clone(), "fresh-bbb".to_string())));

        // excluding both → no false fallback to the stale same-cwd rollout
        let both: HashSet<PathBuf> = [stale, fresh].into_iter().collect();
        assert!(locate_new_in(&root, Path::new("/w"), &both, None).is_none());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn locate_by_session_id_matches_filename_suffix() {
        let root = test_root("by-sid");
        let day = root.join("2026/06/27");
        write_rollout(&day, "aaa-111", "/w", "x");
        let target = write_rollout(&day, "bbb-222", "/w", "y");
        assert_eq!(locate_by_session_id_in(&root, "bbb-222"), Some(target));
        assert_eq!(locate_by_session_id_in(&root, "no-such-sid"), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn locate_by_nonce_picks_the_marked_rollout() {
        let root = test_root("by-nonce");
        let day = root.join("2026/06/27");
        // identical cwd on both — only the nonce disambiguates
        write_rollout(&day, "aaa", "/w", "an unrelated codex run");
        let marked = write_rollout(&day, "bbb", "/w", "do it [dispatch-task: d-7:NONCE42]");
        let got = locate_by_nonce_in(&root, Path::new("/w"), "d-7:NONCE42");
        assert_eq!(got, Some((marked, "bbb".to_string())));
        assert!(locate_by_nonce_in(&root, Path::new("/w"), "absent").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
