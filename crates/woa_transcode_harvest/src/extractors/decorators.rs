//! Decorator classifier — heuristic match on the raw `@<expr>` text.
//! Phase 1 will replace this with a proper AST visit + name resolution
//! via `ruff_python_semantic`.

use crate::bundle::DecoratorKind;

pub fn classify(raw: &str) -> DecoratorKind {
    // strip leading '@' and any whitespace
    let s = raw.trim_start_matches('@').trim();
    // strip arguments — `bp.route('/x')` -> `bp.route`
    let head = s.split('(').next().unwrap_or(s);

    if head.ends_with(".route") || head == "route" {
        return DecoratorKind::Route;
    }
    if matches!(
        head,
        "login_required"
            | "admin_required"
            | "superadmin_required"
            | "require_admin"
            | "require_admin_hierarchy"
    ) {
        return DecoratorKind::Auth;
    }
    if head == "require_perm" || head == "require_scope_via_workorder" {
        return DecoratorKind::Scope;
    }
    if head == "modul_required" || head == "woa_service_required" {
        return DecoratorKind::ModuleRequired;
    }
    DecoratorKind::Other
}
