//! Decorator classifier — heuristic match on the raw `@<expr>` text.
//!
//! Used by the legacy `harvest_module` API for backwards compatibility.
//! New code should drive classification from `config.match[].kind` instead
//! of relying on this hardcoded list.

use crate::bundle::DecoratorKind;

pub fn classify(raw: &str) -> DecoratorKind {
    // strip leading '@' and any whitespace
    let s = raw.trim_start_matches('@').trim();
    // strip arguments — `bp.route('/x')` -> `bp.route`
    let head = s.split('(').next().unwrap_or(s);

    // Matching Python decorator attribute name, not a file extension.
    #[expect(clippy::case_sensitive_file_extension_comparisons)]
    let is_route = head.ends_with(".route") || head == "route";
    if is_route {
        return DecoratorKind::Route;
    }
    if matches!(head, "login_required" | "admin_required") {
        return DecoratorKind::Auth;
    }
    DecoratorKind::Other
}
