use anyhow::Result;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{self as ast, Expr, Operator};
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::Ranged;

use crate::Violation;
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
    {
        if let Some(mode_arg) = call.arguments.find_argument_value("mode", 1) {
            match analyze_mask(mode_arg, checker.semantic()) {
                Ok(analysis) => {
                    if analysis.bits.must & DANGEROUS_MASK > 0 {
                        checker.report_diagnostic(
                            BadFilePermissions {
                                reason: Reason::Permissive(
                                    analysis
                                        .value
                                        .unwrap_or(analysis.bits.must),
                                ),
                            },
                            mode_arg.range(),
                        );
                    } else if analysis
                        .value
                        .is_some_and(|value| value > MAX_PERMISSION_MASK)
                    {
                        checker.report_diagnostic(
                            BadFilePermissions {
                                reason: Reason::Invalid,
                            },
                            mode_arg.range(),
                        );
                    }
                }
                // The mask is an invalid integer value (i.e., it's out of range).
                Err(_) => {
                    checker.report_diagnostic(
                        BadFilePermissions {
                            reason: Reason::Invalid,
                        },
                        mode_arg.range(),
                    );
                }
            }
        }
    }
}

const EXECUTE_WORLD: u64 = 0o1;
const WRITE_WORLD: u64 = 0o2;
const EXECUTE_GROUP: u64 = 0o10;
const WRITE_GROUP: u64 = 0o20;
const DANGEROUS_MASK: u64 = EXECUTE_WORLD | WRITE_WORLD | EXECUTE_GROUP | WRITE_GROUP;
const MAX_PERMISSION_MASK: u64 = 0o7777;

#[derive(Debug, Clone, Copy)]
struct BitAnalysis {
    /// Bits that are always set.
    must: u64,
    /// Bits that may be set.
    may: u64,
}

impl BitAnalysis {
    const fn known(value: u64) -> Self {
        Self {
            must: value,
            may: value,
        }
    }

    const fn unknown() -> Self {
        Self {
            must: 0,
            may: u64::MAX,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct MaskAnalysis {
    /// Exact known mask value if it can be statically determined.
    value: Option<u64>,
    /// Bit analysis for partially-known expressions.
    bits: BitAnalysis,
}

impl MaskAnalysis {
    const fn known(value: u64) -> Self {
        Self {
            value: Some(value),
            bits: BitAnalysis::known(value),
        }
    }

    const fn unknown() -> Self {
        Self {
            value: None,
            bits: BitAnalysis::unknown(),
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

/// Return a partial or exact analysis of the mask expression.
///
/// For unknown expressions, this function returns [`MaskAnalysis::unknown`]. Returns an error if
/// an integer literal exists but cannot be represented in a `u64`.
fn analyze_mask(expr: &Expr, semantic: &SemanticModel) -> Result<MaskAnalysis> {
    match expr {
        Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(int),
            ..
        }) => match int.as_u64() {
            Some(value) => Ok(MaskAnalysis::known(value)),
            None => anyhow::bail!("int value out of range"),
        },
        Expr::Attribute(_) => Ok(semantic
            .resolve_qualified_name(expr)
            .as_ref()
            .and_then(py_stat)
            .map_or_else(MaskAnalysis::unknown, MaskAnalysis::known)),
        Expr::BinOp(ast::ExprBinOp {
            left,
            op,
            right,
            range: _,
            node_index: _,
        }) => {
            let left_analysis = analyze_mask(left, semantic)?;
            let right_analysis = analyze_mask(right, semantic)?;

            Ok(match op {
                Operator::BitAnd => MaskAnalysis {
                    value: left_analysis
                        .value
                        .zip(right_analysis.value)
                        .map(|(left_value, right_value)| left_value & right_value),
                    bits: BitAnalysis {
                        must: left_analysis.bits.must & right_analysis.bits.must,
                        may: left_analysis.bits.may & right_analysis.bits.may,
                    },
                },
                Operator::BitOr => MaskAnalysis {
                    value: left_analysis
                        .value
                        .zip(right_analysis.value)
                        .map(|(left_value, right_value)| left_value | right_value),
                    bits: BitAnalysis {
                        must: left_analysis.bits.must | right_analysis.bits.must,
                        may: left_analysis.bits.may | right_analysis.bits.may,
                    },
                },
                Operator::BitXor => MaskAnalysis {
                    value: left_analysis
                        .value
                        .zip(right_analysis.value)
                        .map(|(left_value, right_value)| left_value ^ right_value),
                    bits: BitAnalysis {
                        must: (left_analysis.bits.must & !right_analysis.bits.may)
                            | (!left_analysis.bits.may & right_analysis.bits.must),
                        may: (left_analysis.bits.may & !right_analysis.bits.must)
                            | (!left_analysis.bits.must & right_analysis.bits.may),
                    },
                },
                _ => MaskAnalysis::unknown(),
            })
        }
        _ => Ok(MaskAnalysis::unknown()),
    }
}
