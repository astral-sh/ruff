use num_traits::ToPrimitive;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::CallPath;
use ruff_python_ast::{self as ast, Constant, Expr, Operator};
use ruff_python_semantic::SemanticModel;
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
#[violation]
pub struct BadFilePermissions {
    mask: u16,
}

impl Violation for BadFilePermissions {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadFilePermissions { mask } = self;
        format!("`os.chmod` setting a permissive mask `{mask:#o}` on file or directory")
    }
}

/// S103
pub(crate) fn bad_file_permissions(checker: &mut Checker, call: &ast::ExprCall) {
    if checker
        .semantic()
        .resolve_call_path(&call.func)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["os", "chmod"]))
    {
        if let Some(mode_arg) = call.arguments.find_argument("mode", 1) {
            if let Some(int_value) = int_value(mode_arg, checker.semantic()) {
                if (int_value & WRITE_WORLD > 0) || (int_value & EXECUTE_GROUP > 0) {
                    checker.diagnostics.push(Diagnostic::new(
                        BadFilePermissions { mask: int_value },
                        mode_arg.range(),
                    ));
                }
            }
        }
    }
}

const WRITE_WORLD: u16 = 0o2;
const EXECUTE_GROUP: u16 = 0o10;

fn py_stat(call_path: &CallPath) -> Option<u16> {
    match call_path.as_slice() {
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

fn int_value(expr: &Expr, semantic: &SemanticModel) -> Option<u16> {
    match expr {
        Expr::Constant(ast::ExprConstant {
            value: Constant::Int(value),
            ..
        }) => value.to_u16(),
        Expr::Attribute(_) => semantic.resolve_call_path(expr).as_ref().and_then(py_stat),
        Expr::BinOp(ast::ExprBinOp {
            left,
            op,
            right,
            range: _,
        }) => {
            let left_value = int_value(left, semantic)?;
            let right_value = int_value(right, semantic)?;
            match op {
                Operator::BitAnd => Some(left_value & right_value),
                Operator::BitOr => Some(left_value | right_value),
                Operator::BitXor => Some(left_value ^ right_value),
                _ => None,
            }
        }
        _ => None,
    }
}
