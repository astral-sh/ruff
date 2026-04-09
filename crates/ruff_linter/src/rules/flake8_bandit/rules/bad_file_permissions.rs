use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{self as ast, Expr, Operator};
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::preview::is_s103_extended_dangerous_bits_enabled;

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
/// ## Preview
/// When [preview] is enabled, the set of bits treated as dangerous matches
/// upstream Bandit (`0o33`): `S_IWOTH`, `S_IXOTH`, `S_IWGRP`, and `S_IXGRP`.
/// Outside preview, only `S_IWOTH` and `S_IXGRP` are flagged.
///
/// [preview]: https://docs.astral.sh/ruff/preview/
///
/// ## References
/// - [Python documentation: `os.chmod`](https://docs.python.org/3/library/os.html#os.chmod)
/// - [Python documentation: `stat`](https://docs.python.org/3/library/stat.html)
/// - [Common Weakness Enumeration: CWE-732](https://cwe.mitre.org/data/definitions/732.html)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.211")]
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
    Permissive(u64),
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
        && let Some(mode_arg) = call.arguments.find_argument_value("mode", 1)
    {
        let known = parse_mask(mode_arg, checker.semantic());
        let dangerous = if is_s103_extended_dangerous_bits_enabled(checker.settings()) {
            DANGEROUS_BITS_PREVIEW
        } else {
            DANGEROUS_BITS_STABLE
        };

        // Prefer `Invalid` over `Permissive` to match the legacy behavior
        // where an out-of-range integer short-circuited the permissiveness
        // check.
        if known.oversized || known.ones & !VALID_BITS != 0 {
            checker.report_diagnostic(
                BadFilePermissions {
                    reason: Reason::Invalid,
                },
                mode_arg.range(),
            );
        } else if known.is_fully_known() && known.ones & dangerous != 0 {
            checker.report_diagnostic(
                BadFilePermissions {
                    reason: Reason::Permissive(known.ones),
                },
                mode_arg.range(),
            );
        }
    }
}

/// World-writable (`S_IWOTH = 0o2`) and group-executable (`S_IXGRP = 0o10`).
const DANGEROUS_BITS_STABLE: u64 = 0o12;
/// Upstream Bandit's full dangerous-bit mask: `S_IWOTH | S_IXOTH | S_IWGRP | S_IXGRP`.
const DANGEROUS_BITS_PREVIEW: u64 = 0o33;
/// The 12 bits that make up a valid Unix permission mask: the rwx-triplets
/// for user/group/other plus setuid, setgid, and the sticky bit.
const VALID_BITS: u64 = 0o7777;

/// Known-bits abstract value for a `u64`: `ones` are the bits that are
/// statically known to be 1, `zeros` are the bits that are statically known
/// to be 0. An expression whose value is fully known satisfies
/// `!oversized && ones | zeros == u64::MAX`; an expression whose value is
/// entirely unknown has `ones == 0 && zeros == 0 && !oversized`.
///
/// The `oversized` flag indicates that the value is known to exceed `u64::MAX`
/// (i.e., it has bits set above bit 63). This is tracked separately because we
/// cannot represent those high bits in a `u64`.
#[derive(Copy, Clone)]
struct KnownBits {
    ones: u64,
    zeros: u64,
    oversized: bool,
}

impl KnownBits {
    const fn exact(value: u64) -> Self {
        Self {
            ones: value,
            zeros: !value,
            oversized: false,
        }
    }

    const fn unknown() -> Self {
        Self {
            ones: 0,
            zeros: 0,
            oversized: false,
        }
    }

    const fn is_fully_known(&self) -> bool {
        !self.oversized && self.ones | self.zeros == u64::MAX
    }

    const fn invalid() -> Self {
        Self {
            ones: 0,
            zeros: 0,
            oversized: true,
        }
    }
}

fn py_stat(qualified_name: &QualifiedName) -> Option<u64> {
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

/// Partially evaluate `expr` as a mask expression, tracking which bits are
/// statically known to be 0 or 1. Sub-expressions that cannot be analyzed
/// contribute no information (all bits unknown).
fn parse_mask(expr: &Expr, semantic: &SemanticModel) -> KnownBits {
    match expr {
        Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(int),
            ..
        }) => int
            .as_u64()
            .map(KnownBits::exact)
            .unwrap_or_else(KnownBits::invalid),
        Expr::Attribute(_) => semantic
            .resolve_qualified_name(expr)
            .as_ref()
            .and_then(py_stat)
            .map(KnownBits::exact)
            .unwrap_or_else(KnownBits::unknown),
        Expr::BinOp(ast::ExprBinOp {
            left, op, right, ..
        }) => {
            let left_bits = parse_mask(left, semantic);
            let right_bits = parse_mask(right, semantic);
            match op {
                Operator::BitOr => KnownBits {
                    ones: left_bits.ones | right_bits.ones,
                    zeros: left_bits.zeros & right_bits.zeros,
                    oversized: left_bits.oversized || right_bits.oversized,
                },
                Operator::BitAnd => KnownBits {
                    ones: left_bits.ones & right_bits.ones,
                    zeros: left_bits.zeros | right_bits.zeros,
                    oversized: left_bits.oversized && right_bits.oversized,
                },
                Operator::BitXor => KnownBits {
                    ones: (left_bits.ones & right_bits.zeros) | (left_bits.zeros & right_bits.ones),
                    zeros: (left_bits.ones & right_bits.ones)
                        | (left_bits.zeros & right_bits.zeros),
                    oversized: left_bits.oversized || right_bits.oversized,
                },
                _ => KnownBits::unknown(),
            }
        }
        _ => KnownBits::unknown(),
    }
}
