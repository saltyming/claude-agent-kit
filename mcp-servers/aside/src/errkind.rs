//! Classification of post-spawn backend failures into "transient, fallback-worthy"
//! vs "permanent" — the basis for the model_fallback retry loop.
//!
//! Duplicated verbatim in mcp-servers/dispatch/src/errkind.rs, matching this
//! codebase's existing convention for small shared utility modules (see
//! lenient.rs) rather than adding a shared workspace crate for ~100 lines.
//!
//! The pattern table was tuned against REAL captured stderr from all three
//! backends (codex via both aside and dispatch, copilot, opencode), not
//! guessed:
//!   - codex, bad model, ChatGPT-account auth: a JSON body containing
//!     `"type":"invalid_request_error"` and a message like "The 'X' model is
//!     not supported when using Codex with a ChatGPT account." — note this
//!     does NOT contain the substring "unsupported model"; the patterns below
//!     match on "not supported" instead.
//!   - copilot, policy-denied access: "Access denied by policy settings" — no
//!     401/403/"unauthorized" text at all.
//!   - opencode, bad model: a generic, non-discriminating HTTP 500 —
//!     `{"name":"UnknownError","data":{"message":"Unexpected server error.
//!     Check server logs for details."}}`. There is no text to pattern-match
//!     against for this failure class; opencode failures of this kind will
//!     classify as `Other` (not retried). This is a real, accepted limitation,
//!     not an oversight — see the module-level doc on `classify`.

/// Backend/model failure classification. `Other` is the default-deny bucket:
/// anything that doesn't positively match a known transient shape is treated
/// as permanent (would fail identically on any model) and is never retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendErrorKind {
    RateLimited,
    QuotaOrBilling,
    ModelUnavailable,
    AuthOrPermission,
    Other,
}

impl BackendErrorKind {
    /// `AuthOrPermission` is deliberately NOT retry-worthy: a `model_fallback`
    /// chain swaps models *within* a single already-chosen backend/account
    /// (dispatch's `Job.backend`, aside's `backend` param each pick codex vs.
    /// opencode/copilot once, before any fallback runs), so an auth/permission
    /// failure affects that whole account and would fail identically on every
    /// model in the chain — retrying just burns the rest of the chain for no
    /// benefit.
    pub fn is_retry_worthy(self) -> bool {
        matches!(
            self,
            BackendErrorKind::RateLimited
                | BackendErrorKind::QuotaOrBilling
                | BackendErrorKind::ModelUnavailable
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            BackendErrorKind::RateLimited => "rate_limited",
            BackendErrorKind::QuotaOrBilling => "quota_or_billing",
            BackendErrorKind::ModelUnavailable => "model_unavailable",
            BackendErrorKind::AuthOrPermission => "auth_or_permission",
            BackendErrorKind::Other => "other",
        }
    }
}

const TRANSIENT_PATTERNS: &[(&str, BackendErrorKind)] = &[
    // Rate limiting.
    ("rate_limit_exceeded", BackendErrorKind::RateLimited),
    ("rate limit", BackendErrorKind::RateLimited),
    ("too many requests", BackendErrorKind::RateLimited),
    ("429", BackendErrorKind::RateLimited),
    // Quota / billing.
    ("insufficient_quota", BackendErrorKind::QuotaOrBilling),
    ("quota", BackendErrorKind::QuotaOrBilling),
    ("insufficient credits", BackendErrorKind::QuotaOrBilling),
    ("billing", BackendErrorKind::QuotaOrBilling),
    ("payment required", BackendErrorKind::QuotaOrBilling),
    ("402", BackendErrorKind::QuotaOrBilling),
    // Model unavailable / unknown / not supported for this account.
    ("model_not_found", BackendErrorKind::ModelUnavailable),
    ("model not found", BackendErrorKind::ModelUnavailable),
    (
        "does not exist or you do not have access",
        BackendErrorKind::ModelUnavailable,
    ),
    ("unknown model", BackendErrorKind::ModelUnavailable),
    ("unsupported model", BackendErrorKind::ModelUnavailable),
    // Real codex shape (ChatGPT-account auth): "The 'X' model is not
    // supported when using Codex with a ChatGPT account." — captured live,
    // does not contain "unsupported model".
    ("not supported", BackendErrorKind::ModelUnavailable),
    ("overloaded_error", BackendErrorKind::ModelUnavailable),
    ("overloaded", BackendErrorKind::ModelUnavailable),
    ("currently unavailable", BackendErrorKind::ModelUnavailable),
    ("503", BackendErrorKind::ModelUnavailable),
    ("502", BackendErrorKind::ModelUnavailable),
    ("504", BackendErrorKind::ModelUnavailable),
    // Auth / permission — NOT retry-worthy (see is_retry_worthy), but still
    // classified distinctly from Other so it's diagnosable in logs/history.
    ("invalid_api_key", BackendErrorKind::AuthOrPermission),
    ("invalid api key", BackendErrorKind::AuthOrPermission),
    ("unauthorized", BackendErrorKind::AuthOrPermission),
    ("authentication", BackendErrorKind::AuthOrPermission),
    ("401", BackendErrorKind::AuthOrPermission),
    ("403", BackendErrorKind::AuthOrPermission),
    ("forbidden", BackendErrorKind::AuthOrPermission),
    // Real copilot shape: "Access denied by policy settings" — no
    // 401/403/"unauthorized" text at all.
    ("access denied", BackendErrorKind::AuthOrPermission),
    ("policy setting", BackendErrorKind::AuthOrPermission),
];

/// Case-insensitive substring match, first hit in table order wins; no match
/// => `Other`.
///
/// KNOWN LIMITATION: opencode's bad-model failure surfaces as a completely
/// generic, non-discriminating HTTP 500 (`"UnknownError"` / "Unexpected
/// server error. Check server logs for details.") with no text this function
/// can key on. That failure class will classify as `Other` and will not be
/// retried — this is an accepted limitation of the opencode backend's error
/// surface, not a bug in this matcher. If opencode's rate-limit/quota errors
/// surface more specific text than its bad-model errors do, those may still
/// classify correctly; only the bad-model case is confirmed opaque.
pub fn classify(text: &str) -> BackendErrorKind {
    let lower = text.to_ascii_lowercase();
    for (pat, kind) in TRANSIENT_PATTERNS {
        if lower.contains(pat) {
            return *kind;
        }
    }
    BackendErrorKind::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_real_captured_codex_bad_model_shape() {
        // Captured live from both aside_codex and dispatch (codex backend),
        // ChatGPT-account auth, 2026-07-02.
        let text = r#"ERROR: {"type":"error","status":400,"error":{"type":"invalid_request_error","message":"The 'definitely-not-a-real-model-xyz-9999' model is not supported when using Codex with a ChatGPT account."}}"#;
        assert_eq!(classify(text), BackendErrorKind::ModelUnavailable);
    }

    #[test]
    fn recognizes_real_captured_copilot_policy_denial_shape() {
        // Captured live from aside_copilot, 2026-07-02.
        let text = "Error: Access denied by policy settings (Request ID: E2E0:196D8B:1FB4EB4:22A7BE4:6A462B9F)\n\nYour Copilot CLI policy setting may be preventing access.";
        let kind = classify(text);
        assert_eq!(kind, BackendErrorKind::AuthOrPermission);
        assert!(!kind.is_retry_worthy());
    }

    #[test]
    fn opencode_generic_500_is_unclassifiable_by_design() {
        // Captured live from dispatch (opencode backend), bad model,
        // 2026-07-02 — confirms the documented limitation: no distinguishing
        // text, so this correctly (if unfortunately) falls through to Other.
        let text = r#"opencode API returned 500 Internal Server Error: {"name":"UnknownError","data":{"message":"Unexpected server error. Check server logs for details.","ref":"err_7a99a2c3"}}"#;
        assert_eq!(classify(text), BackendErrorKind::Other);
    }

    #[test]
    fn recognizes_other_known_transient_shapes() {
        assert_eq!(
            classify("Error: rate_limit_exceeded"),
            BackendErrorKind::RateLimited
        );
        assert_eq!(
            classify("429 Too Many Requests"),
            BackendErrorKind::RateLimited
        );
        assert_eq!(
            classify("You exceeded your current quota"),
            BackendErrorKind::QuotaOrBilling
        );
        assert_eq!(
            classify("model_not_found: gpt-99 does not exist"),
            BackendErrorKind::ModelUnavailable
        );
        assert_eq!(
            classify("503 Service Unavailable: overloaded_error"),
            BackendErrorKind::ModelUnavailable
        );
        assert_eq!(
            classify("401 Unauthorized: invalid api key"),
            BackendErrorKind::AuthOrPermission
        );
    }

    #[test]
    fn defaults_to_other_for_unrecognized_text() {
        assert_eq!(
            classify("panicked at src/main.rs:42"),
            BackendErrorKind::Other
        );
        assert_eq!(
            classify("permission denied: cannot write /etc/passwd"),
            BackendErrorKind::Other
        );
        assert_eq!(classify(""), BackendErrorKind::Other);
    }

    #[test]
    fn is_case_insensitive() {
        assert_eq!(
            classify("RATE LIMIT EXCEEDED"),
            BackendErrorKind::RateLimited
        );
    }

    #[test]
    fn is_retry_worthy_excludes_auth_and_other() {
        assert!(BackendErrorKind::RateLimited.is_retry_worthy());
        assert!(BackendErrorKind::QuotaOrBilling.is_retry_worthy());
        assert!(BackendErrorKind::ModelUnavailable.is_retry_worthy());
        assert!(!BackendErrorKind::AuthOrPermission.is_retry_worthy());
        assert!(!BackendErrorKind::Other.is_retry_worthy());
    }
}
