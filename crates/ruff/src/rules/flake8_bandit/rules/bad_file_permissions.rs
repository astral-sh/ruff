use num_traits::ToPrimitive;
use once_cell::sync::Lazy;
use ruff_macros::{define_violation, derive_message_formats};
use rustc_hash::FxHashMap;
use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword, Operator};

use crate::ast::helpers::{compose_call_path, SimpleCallArgs};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct BadFilePermissions {
        pub mask: u16,
    }
);
impl Violation for BadFilePermissions {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadFilePermissions { mask } = self;
        format!("`os.chmod` setting a permissive mask `{mask:#o}` on file or directory",)
    }
}

const WRITE_WORLD: u16 = 0o2;
const EXECUTE_GROUP: u16 = 0o10;

static PYSTAT_MAPPING: Lazy<FxHashMap<&'static str, u16>> = Lazy::new(|| {
    FxHashMap::from_iter([
        ("stat.ST_MODE", 0o0),
        ("stat.S_IFDOOR", 0o0),
        ("stat.S_IFPORT", 0o0),
        ("stat.ST_INO", 0o1),
        ("stat.S_IXOTH", 0o1),
        ("stat.UF_NODUMP", 0o1),
        ("stat.ST_DEV", 0o2),
        ("stat.S_IWOTH", 0o2),
        ("stat.UF_IMMUTABLE", 0o2),
        ("stat.ST_NLINK", 0o3),
        ("stat.ST_UID", 0o4),
        ("stat.S_IROTH", 0o4),
        ("stat.UF_APPEND", 0o4),
        ("stat.ST_GID", 0o5),
        ("stat.ST_SIZE", 0o6),
        ("stat.ST_ATIME", 0o7),
        ("stat.S_IRWXO", 0o7),
        ("stat.ST_MTIME", 0o10),
        ("stat.S_IXGRP", 0o10),
        ("stat.UF_OPAQUE", 0o10),
        ("stat.ST_CTIME", 0o11),
        ("stat.S_IWGRP", 0o20),
        ("stat.UF_NOUNLINK", 0o20),
        ("stat.S_IRGRP", 0o40),
        ("stat.UF_COMPRESSED", 0o40),
        ("stat.S_IRWXG", 0o70),
        ("stat.S_IEXEC", 0o100),
        ("stat.S_IXUSR", 0o100),
        ("stat.S_IWRITE", 0o200),
        ("stat.S_IWUSR", 0o200),
        ("stat.S_IREAD", 0o400),
        ("stat.S_IRUSR", 0o400),
        ("stat.S_IRWXU", 0o700),
        ("stat.S_ISVTX", 0o1000),
        ("stat.S_ISGID", 0o2000),
        ("stat.S_ENFMT", 0o2000),
        ("stat.S_ISUID", 0o4000),
    ])
});

fn get_int_value(expr: &Expr) -> Option<u16> {
    match &expr.node {
        ExprKind::Constant {
            value: Constant::Int(value),
            ..
        } => value.to_u16(),
        ExprKind::Attribute { .. } => {
            compose_call_path(expr).and_then(|path| PYSTAT_MAPPING.get(path.as_str()).copied())
        }
        ExprKind::BinOp { left, op, right } => {
            if let (Some(left_value), Some(right_value)) =
                (get_int_value(left), get_int_value(right))
            {
                match op {
                    Operator::BitAnd => Some(left_value & right_value),
                    Operator::BitOr => Some(left_value | right_value),
                    Operator::BitXor => Some(left_value ^ right_value),
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

/// S103
pub fn bad_file_permissions(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if checker
        .resolve_call_path(func)
        .map_or(false, |call_path| call_path.as_slice() == ["os", "chmod"])
    {
        let call_args = SimpleCallArgs::new(args, keywords);
        if let Some(mode_arg) = call_args.get_argument("mode", Some(1)) {
            if let Some(int_value) = get_int_value(mode_arg) {
                if (int_value & WRITE_WORLD > 0) || (int_value & EXECUTE_GROUP > 0) {
                    checker.diagnostics.push(Diagnostic::new(
                        BadFilePermissions { mask: int_value },
                        Range::from_located(mode_arg),
                    ));
                }
            }
        }
    }
}
