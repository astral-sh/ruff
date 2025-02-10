use anyhow::Result;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{self as ast, Expr, Operator};
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for files with overly permissive permissions.
///
/// ## Why is this bad?
/// Overly permissive file permissions may allow unintended access and
/// arbitrary code execution.
///
/// ## Example
/// ```python
/// import os
///
/// os.chmod("/etc/secrets.txt", 0o666)  # rw-rw-rw-
/// ```
///
/// Use instead:
/// ```python
/// import os
///
/// os.chmod("/etc/secrets.txt", 0o600)  # rw-------
/// ```
///
/// ## References
/// - [Python documentation: `os.chmod`](https://docs.python.org/3/library/os.html#os.chmod)
/// - [Python documentation: `stat`](https://docs.python.org/3/library/stat.html)
/// - [Common Weakness Enumeration: CWE-732](https://cwe.mitre.org/data/definitions/732.html)
#[derive(ViolationMetadata)]
pub(crate) struct BadFilePermissions {
    reason: Reason,
}

impl Violation for BadFilePermissions {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadFilePermissions { reason } = self;
        match reason {
            Reason::Permissive(mask) => {
                format!("`os.chmod` setting a permissive mask `{mask:#o}` on file or directory")
            }
            Reason::Invalid => {
                "`os.chmod` setting an invalid mask on file or directory".to_string()
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Reason {
    Permissive(u16),
    Invalid,
}

/// S103
pub(crate) fn bad_file_permissions(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::OS) {
        return;
    }

    if checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["os", "chmod"]))
    {
        if let Some(mode_arg) = call.arguments.find_argument_value("mode", 1) {
            match parse_mask(mode_arg, checker.semantic()) {
                // The mask couldn't be determined (e.g., it's dynamic).
                Ok(None) => {}
                // The mask is a valid integer value -- check for overly permissive permissions.
                Ok(Some(mask)) => {
                    if (mask & WRITE_WORLD > 0) || (mask & EXECUTE_GROUP > 0) {
                        checker.report_diagnostic(Diagnostic::new(
                            BadFilePermissions {
                                reason: Reason::Permissive(mask),
                            },
                            mode_arg.range(),
                        ));
                    }
                }
                // The mask is an invalid integer value (i.e., it's out of range).
                Err(_) => {
                    checker.report_diagnostic(Diagnostic::new(
                        BadFilePermissions {
                            reason: Reason::Invalid,
                        },
                        mode_arg.range(),
                    ));
                }
            }
        }
    }
}

const WRITE_WORLD: u16 = 0o2;
const EXECUTE_GROUP: u16 = 0o10;

fn py_stat(qualified_name: &QualifiedName) -> Option<u16> {
    match qualified_name.segments() {
        ["stat", "ST_MODE"] => Some(0o0),
        ["stat", "S_IFDOOR"] => Some(0o0),
        ["stat", "S_IFPORT"] => Some(0o0),
        ["stat", "ST_INO"] => Some(0o1),
        ["stat", "S_IXOTH"] => Some(0o1),
        ["stat", "UF_NODUMP"] => Some(0o1),
        ["stat", "ST_DEV"] => Some(0o2),
        ["stat", "S_IWOTH"] => Some(0o2),
        ["stat", "UF_IMMUTABLE"] => Some(0o2),
        ["stat", "ST_NLINK"] => Some(0o3),
        ["stat", "ST_UID"] => Some(0o4),
        ["stat", "S_IROTH"] => Some(0o4),
        ["stat", "UF_APPEND"] => Some(0o4),
        ["stat", "ST_GID"] => Some(0o5),
        ["stat", "ST_SIZE"] => Some(0o6),
        ["stat", "ST_ATIME"] => Some(0o7),
        ["stat", "S_IRWXO"] => Some(0o7),
        ["stat", "ST_MTIME"] => Some(0o10),
        ["stat", "S_IXGRP"] => Some(0o10),
        ["stat", "UF_OPAQUE"] => Some(0o10),
        ["stat", "ST_CTIME"] => Some(0o11),
        ["stat", "S_IWGRP"] => Some(0o20),
        ["stat", "UF_NOUNLINK"] => Some(0o20),
        ["stat", "S_IRGRP"] => Some(0o40),
        ["stat", "UF_COMPRESSED"] => Some(0o40),
        ["stat", "S_IRWXG"] => Some(0o70),
        ["stat", "S_IEXEC"] => Some(0o100),
        ["stat", "S_IXUSR"] => Some(0o100),
        ["stat", "S_IWRITE"] => Some(0o200),
        ["stat", "S_IWUSR"] => Some(0o200),
        ["stat", "S_IREAD"] => Some(0o400),
        ["stat", "S_IRUSR"] => Some(0o400),
        ["stat", "S_IRWXU"] => Some(0o700),
        ["stat", "S_ISVTX"] => Some(0o1000),
        ["stat", "S_ISGID"] => Some(0o2000),
        ["stat", "S_ENFMT"] => Some(0o2000),
        ["stat", "S_ISUID"] => Some(0o4000),
        _ => None,
    }
}

/// Return the mask value as a `u16`, if it can be determined. Returns an error if the mask is
/// an integer value, but that value is out of range.
fn parse_mask(expr: &Expr, semantic: &SemanticModel) -> Result<Option<u16>> {
    match expr {
        Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(int),
            ..
        }) => match int.as_u16() {
            Some(value) => Ok(Some(value)),
            None => anyhow::bail!("int value out of range"),
        },
        Expr::Attribute(_) => Ok(semantic
            .resolve_qualified_name(expr)
            .as_ref()
            .and_then(py_stat)),
        Expr::BinOp(ast::ExprBinOp {
            left,
            op,
            right,
            range: _,
        }) => {
            let Some(left_value) = parse_mask(left, semantic)? else {
                return Ok(None);
            };
            let Some(right_value) = parse_mask(right, semantic)? else {
                return Ok(None);
            };
            Ok(match op {
                Operator::BitAnd => Some(left_value & right_value),
                Operator::BitOr => Some(left_value | right_value),
                Operator::BitXor => Some(left_value ^ right_value),
                _ => None,
            })
        }
        _ => Ok(None),
    }
}
