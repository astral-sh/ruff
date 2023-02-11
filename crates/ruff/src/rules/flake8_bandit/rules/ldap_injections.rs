use once_cell::sync::Lazy;
use regex::Regex;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind, Operator};

use super::super::helpers::string_literal;
use crate::ast::helpers::{any_over_expr, unparse_expr};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

enum FormatSpecifier {
    None,
    Percent,
    Bracket,
}

static RE_LDAP_FILTER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)^[frub]*['"]+\(\s*[&!|]\s*\(.*="#).unwrap());

static RE_LDAP_FILTER_COMMON: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)^[frub]*['"]+\(\s*(dc|cn|c|o|l|uid)\s*.*=.*\)"#).unwrap());

static RE_LDAP_DN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)^[frub]*['"]+\s*([a-z][a-z0-9-]*\s*=[^,]*,)*\s*((?:dc|cn|c|o|l)\s*=[^,]*)*(?:,|\s*['"]+$)"#)
        .unwrap()
});
static RE_LDAP_DN_SEGMENTS: Lazy<Regex> = Lazy::new(|| {
    // FIXME: error: look-around, including look-ahead and look-behind, is not supported
    // Regex::new(r#"(?<!\\),"#).unwrap()
    Regex::new(r",").unwrap()
});
static RE_FORMAT_SPECIFIER_PERCENT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)^[^=]+=.*[%]"#).unwrap());

static RE_FORMAT_SPECIFIER_BRACKET: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)^[^=]+=.*[{]"#).unwrap());

define_violation!(
    /// ## What it does
    /// Checks for strings that allow LDAP filter or DN injections by not
    /// using a escape function.
    ///
    /// ## Why is this bad?
    /// LDAP injection is a common attack vector for LDAP based applications. Directly
    /// interpolating user input into LDAP filters or DNs should always be avoided.
    ///
    /// ## Example
    /// ```python
    /// filter = '(&(objectClass=person)(uid=%s)' % (username,)
    /// dn = 'uid=%s,cn=users,dc=base' % (username,)
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// filter = ldap.filter.filter_format('(&(objectClass=person)(uid=%s)', (username,))
    /// dn = 'uid=%s,cn=users,dc=base' % (ldap.dn.escape_dn_chars(username),)
    /// ```
    pub struct LDAPInjection {
        pub injection_type: String,
        pub string: String,
    }
);
impl Violation for LDAPInjection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LDAPInjection {
            injection_type,
            string,
        } = self;
        format!(
            "Possible LDAP injection vector through string-based {} construction: \"{}\"",
            injection_type,
            string.escape_debug()
        )
    }
}

fn uses_escape_function(checker: &Checker, expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Call { func, .. } => {
            checker.resolve_call_path(func).map_or(false, |call_path| {
                // FIXME: respect injection_type
                // ldap.filter.escape_filter_chars
                // ldap.escape_filter_chars
                // ldap3.utils.conv.escape_filter_chars
                // ldap.dn.escape_dn_chars
                // ldap.escape_dn_chars
                call_path.contains(&"escape_filter_chars") || call_path.contains(&"escape_dn_chars")
                //call_path.as_slice() == ["ldap", "filter", "escape_filter_chars"]
            })
        }
        _ => false,
    }
}

fn has_string_literal(expr: &Expr) -> bool {
    string_literal(expr).is_some()
}

fn matches_filter_string(string: &str, _fmt_specifier: &FormatSpecifier) -> bool {
    RE_LDAP_FILTER.is_match(string) || RE_LDAP_FILTER_COMMON.is_match(string)
}

fn matches_dn_string(string: &str, fmt_specifier: &FormatSpecifier) -> bool {
    let mut matched = false;
    if RE_LDAP_DN.is_match(string) {
        for cap in RE_LDAP_DN_SEGMENTS.split(string) {
            if let Some(re) = match fmt_specifier {
                FormatSpecifier::Percent => Some(&RE_FORMAT_SPECIFIER_PERCENT),
                FormatSpecifier::Bracket => Some(&RE_FORMAT_SPECIFIER_BRACKET),
                FormatSpecifier::None => None,
            } {
                if re.is_match(cap) {
                    return true;
                }
            } else {
                matched = true;
            }
        }
    }
    matched
}

fn unparse_string_format_expression(
    checker: &mut Checker,
    expr: &Expr,
) -> Option<(FormatSpecifier, String)> {
    let uses_ldap_escape_function = |expr: &Expr| -> bool { uses_escape_function(checker, expr) };
    // TODO: variable introspection: username = escape(something)
    match &expr.node {
        // "(&(objectClass=person)(uid=" + username + ")"
        // "(&(objectClass=person)(uid=%s)" % (username,)
        ExprKind::BinOp {
            op: Operator::Add | Operator::Mod,
            ..
        } => {
            let fmt_specifier = match &expr.node {
                ExprKind::BinOp {
                    op: Operator::Add, ..
                } => FormatSpecifier::None,
                _ => FormatSpecifier::Percent,
            };
            let Some(parent) = checker.current_expr_parent() else {
                if any_over_expr(expr, &has_string_literal) && !any_over_expr(expr, &uses_ldap_escape_function) {
                    return Some((fmt_specifier, unparse_expr(expr, checker.stylist)));
                }
                return None;
            };
            // Only evaluate the full BinOp, not the nested components.
            let ExprKind::BinOp { .. } = &parent.node else {
                if any_over_expr(expr, &has_string_literal) && !any_over_expr(expr, &uses_ldap_escape_function) {
                    return Some((fmt_specifier, unparse_expr(expr, checker.stylist)));
                }
                return None;
            };
            None
        }
        ExprKind::Call {
            func,
            args,
            keywords,
        } => {
            let ExprKind::Attribute{ attr, value, .. } = &func.node else {
                return None;
            };
            // "(&(objectClass=person)(uid={})".format(username)
            if attr == "format" && string_literal(value).is_some() {
                if args
                    .iter()
                    .any(|arg| any_over_expr(arg, &uses_ldap_escape_function))
                {
                    return None;
                }
                if keywords
                    .iter()
                    .any(|kwarg| any_over_expr(&kwarg.node.value, &uses_ldap_escape_function))
                {
                    return None;
                }
                return Some((
                    FormatSpecifier::Bracket,
                    unparse_expr(expr, checker.stylist),
                ));
            };
            None
        }
        // f"(&(objectClass=person)(uid={username})"
        ExprKind::JoinedStr { values } => {
            if values
                .iter()
                .any(|farg| any_over_expr(farg, &uses_ldap_escape_function))
            {
                return None;
            }
            Some((
                FormatSpecifier::Bracket,
                unparse_expr(expr, checker.stylist),
            ))
        }
        _ => None,
    }
}

/// S613
pub fn ldap_injections(checker: &mut Checker, expr: &Expr) {
    // TODO: only underline the concrete format-specifier or variable instead of the whole string
    match unparse_string_format_expression(checker, expr) {
        Some((fmt_specifier, string)) if matches_filter_string(&string, &fmt_specifier) => {
            let injection_type = "filter".to_string();
            checker.diagnostics.push(Diagnostic::new(
                LDAPInjection {
                    injection_type,
                    string,
                },
                Range::from_located(expr),
            ));
        }
        Some((fmt_specifier, string)) if matches_dn_string(&string, &fmt_specifier) => {
            let injection_type = "DN".to_string();
            checker.diagnostics.push(Diagnostic::new(
                LDAPInjection {
                    injection_type,
                    string,
                },
                Range::from_located(expr),
            ));
        }
        _ => (),
    }
}
