use itertools::Itertools;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{
    BuildId, Builder, CmpOp, Expr, ExprBytesLiteral, ExprCall, ExprKind, ExprStringLiteral,
    UnaryOp, build,
};
use ruff_python_codegen::Generator;
use ruff_python_semantic::analyze::typing::find_binding_value;
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::syntax;
use crate::{Applicability, Edit, Fix, FixAvailability, Violation};

/// ## What it does
///
/// Checks for uses of the `re` module that can be replaced with builtin `str` methods.
///
/// ## Why is this bad?
///
/// Performing checks on strings directly can make the code simpler, may require
/// less escaping, and will often be faster.
///
/// ## Example
///
/// ```python
/// re.sub("abc", "", s)
/// ```
///
/// Use instead:
///
/// ```python
/// s.replace("abc", "")
/// ```
///
/// ## Details
///
/// The rule reports the following calls when the first argument to the call is
/// a plain string literal, and no additional flags are passed:
///
/// - `re.sub`
/// - `re.match`
/// - `re.prefixmatch`
/// - `re.search`
/// - `re.fullmatch`
/// - `re.split`
///
/// For `re.sub`, the `repl` (replacement) argument must also be a string literal,
/// not a function. For `re.match`, `re.prefixmatch`, `re.search`, and `re.fullmatch`,
/// the return value must also be used only for its truth value.
///
/// ## Fix safety
///
/// This rule's fix is marked as unsafe if the affected expression contains comments. Otherwise,
/// the fix can be applied safely.
///
/// ## References
/// - [Python Regular Expression HOWTO: Common Problems - Use String Methods](https://docs.python.org/3/howto/regex.html#use-string-methods)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.8.1", category = Category::Complexity)]
pub(crate) struct UnnecessaryRegularExpression {
    replacement: Option<String>,
}

impl Violation for UnnecessaryRegularExpression {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Plain string pattern passed to `re` function".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Replace with `{}`", self.replacement.as_ref()?))
    }
}

const METACHARACTERS: [char; 12] = ['.', '^', '$', '*', '+', '?', '{', '[', '\\', '|', '(', ')'];
const ESCAPABLE_SINGLE_CHARACTERS: &str = "abfnrtv";

/// RUF055
pub(crate) fn unnecessary_regular_expression(checker: &Checker, call: ExprCall<'_>) {
    // adapted from unraw_re_pattern
    let semantic = checker.semantic();

    if !semantic.seen_module(Modules::RE) {
        return;
    }

    let Some(qualified_name) = semantic.resolve_qualified_name(call.func()) else {
        return;
    };

    let ["re", func] = qualified_name.segments() else {
        return;
    };

    // skip calls with more than `pattern` and `string` arguments (and `repl`
    // for `sub`)
    let Some(re_func) = ReFunc::from_call_expr(semantic, call, func) else {
        return;
    };

    // For now, restrict this rule to string literals and variables that can be resolved to literals
    let Some(literal) = resolve_literal(re_func.pattern, semantic) else {
        return;
    };

    // For now, reject any regex metacharacters. Compare to the complete list
    // from https://docs.python.org/3/howto/regex.html#matching-characters
    let has_metacharacters = match &literal {
        Literal::Str(str_lit) => str_lit.value().to_str().contains(METACHARACTERS),
        Literal::Bytes(bytes_lit) => bytes_lit.value().iter().any(|part| {
            part.value()
                .iter()
                .any(|&b| METACHARACTERS.contains(&(b as char)))
        }),
    };

    if has_metacharacters {
        return;
    }

    // `str.split("")` raises `ValueError: empty separator` while `re.split("", s)` succeeds,
    // so skip the diagnostic for `re.split` with an empty pattern.
    if matches!(re_func.kind, ReFuncKind::Split) && literal.is_empty() {
        return;
    }

    // Now we know the pattern is a string literal with no metacharacters, so
    // we can proceed with the str method replacement.
    let repl = re_func.replacement(checker.generator());
    let mut diagnostic = checker.report_diagnostic(
        UnnecessaryRegularExpression {
            replacement: repl.clone(),
        },
        re_func.range,
    );

    if let Some(repl) = repl {
        diagnostic.set_fix(Fix::applicable_edit(
            Edit::range_replacement(repl, re_func.range),
            if checker.comment_ranges().intersects(re_func.range) {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            },
        ));
    }
}

/// The `re` functions supported by this rule.
#[derive(Debug)]
enum ReFuncKind<'a> {
    // Only `Some` if it's a fixable `re.sub()` call
    Sub { repl: Option<Expr<'a>> },
    Match,
    Search,
    Fullmatch,
    Split,
}

#[derive(Debug)]
struct ReFunc<'a> {
    kind: ReFuncKind<'a>,
    pattern: Expr<'a>,
    string: Expr<'a>,
    comparison_to_none: Option<ComparisonToNone>,
    range: TextRange,
}

impl<'a> ReFunc<'a> {
    fn from_call_expr(
        semantic: &'a SemanticModel,
        call: ExprCall<'a>,
        func_name: &str,
    ) -> Option<Self> {
        // the proposed fixes for match, search, and fullmatch rely on the
        // return value only being used for its truth value or being compared to None
        let comparison_to_none = get_comparison_to_none(semantic);
        let in_truthy_context = semantic.in_boolean_test() || comparison_to_none.is_some();

        let (comparison_to_none, range) = match comparison_to_none {
            Some((cmp, range)) => (Some(cmp), range),
            None => (None, call.range()),
        };

        match (func_name, call.arguments().len()) {
            // `split` is the safest of these to fix, as long as metacharacters
            // have already been filtered out from the `pattern`
            ("split", 2) => Some(ReFunc {
                kind: ReFuncKind::Split,
                pattern: call.arguments().find_argument_value("pattern", 0)?,
                string: call.arguments().find_argument_value("string", 1)?,
                comparison_to_none,
                range,
            }),
            // `sub` is only safe to fix if `repl` is a string. `re.sub` also
            // allows it to be a function, which will *not* work in the str
            // version
            ("sub", 3) => {
                let repl = call.arguments().find_argument_value("repl", 1)?;
                let lit = resolve_literal(repl, semantic)?;
                let mut fixable = true;

                match lit {
                    Literal::Str(lit_str) => {
                        // Perform escape analysis for replacement literals.
                        for (c, next) in lit_str.value().to_str().chars().tuple_windows() {
                            // `\\0` (or any other ASCII digit) and `\\g` have special meaning in `repl` strings.
                            // Meanwhile, nearly all other escapes of ASCII letters in a `repl` string causes
                            // `re.PatternError` to be raised at runtime.
                            //
                            // If we see that the escaped character is an alphanumeric ASCII character,
                            // we should only emit a diagnostic suggesting to replace the `re.sub()` call with
                            // `str.replace`if we can detect that the escaped character is one that is both
                            // valid in a `repl` string *and* does not have any special meaning in a REPL string.
                            //
                            // It's out of scope for this rule to change invalid `re.sub()` calls into something
                            // that would not raise an exception at runtime. They should be left as-is.
                            if c == '\\' && next.is_ascii_alphanumeric() {
                                if ESCAPABLE_SINGLE_CHARACTERS.contains(next) {
                                    fixable = false;
                                } else {
                                    return None;
                                }
                            }
                        }
                    }
                    Literal::Bytes(lit_bytes) => {
                        for part in lit_bytes.value() {
                            for (byte, next) in part.value().iter().copied().tuple_windows() {
                                if byte == b'\\' && (next as char).is_ascii_alphanumeric() {
                                    if ESCAPABLE_SINGLE_CHARACTERS.contains(next as char) {
                                        fixable = false;
                                    } else {
                                        return None;
                                    }
                                }
                            }
                        }
                    }
                }

                Some(ReFunc {
                    kind: ReFuncKind::Sub {
                        repl: fixable.then_some(repl),
                    },
                    pattern: call.arguments().find_argument_value("pattern", 0)?,
                    string: call.arguments().find_argument_value("string", 2)?,
                    comparison_to_none,
                    range,
                })
            }
            ("match" | "prefixmatch", 2) if in_truthy_context => Some(ReFunc {
                kind: ReFuncKind::Match,
                pattern: call.arguments().find_argument_value("pattern", 0)?,
                string: call.arguments().find_argument_value("string", 1)?,
                comparison_to_none,
                range,
            }),
            ("search", 2) if in_truthy_context => Some(ReFunc {
                kind: ReFuncKind::Search,
                pattern: call.arguments().find_argument_value("pattern", 0)?,
                string: call.arguments().find_argument_value("string", 1)?,
                comparison_to_none,
                range,
            }),
            ("fullmatch", 2) if in_truthy_context => Some(ReFunc {
                kind: ReFuncKind::Fullmatch,
                pattern: call.arguments().find_argument_value("pattern", 0)?,
                string: call.arguments().find_argument_value("string", 1)?,
                comparison_to_none,
                range,
            }),
            _ => None,
        }
    }

    /// Get replacement for the call or parent expression.
    ///
    /// Examples:
    ///     `re.search("abc", s) is None` => `"abc" not in s`
    ///     `re.search("abc", s)` => `"abc" in s`
    fn replacement(&self, generator: Generator) -> Option<String> {
        let mut builder = Builder::default();
        let negated = matches!(self.comparison_to_none, Some(ComparisonToNone::Is));
        let replacement = match &self.kind {
            ReFuncKind::Sub { repl } => {
                self.method_expr(&mut builder, "replace", [self.pattern, (*repl)?])
            }
            ReFuncKind::Split => self.method_expr(&mut builder, "split", [self.pattern]),
            ReFuncKind::Match => {
                let starts_with = self.method_expr(&mut builder, "startswith", [self.pattern]);
                if negated {
                    builder
                        .add(build::ExprUnaryOp {
                            range: TextRange::default(),
                            op: UnaryOp::Not,
                            operand: starts_with,
                        })
                        .into()
                } else {
                    starts_with
                }
            }
            ReFuncKind::Search => {
                let pattern = builder.copy_node(self.pattern);
                let string = builder.copy_node(self.string);
                let op = if negated { CmpOp::NotIn } else { CmpOp::In };
                syntax::compare(&mut builder, pattern, op, string)
            }
            ReFuncKind::Fullmatch => {
                let string = builder.copy_node(self.string);
                let pattern = builder.copy_node(self.pattern);
                let op = if negated { CmpOp::NotEq } else { CmpOp::Eq };
                syntax::compare(&mut builder, string, op, pattern)
            }
        };
        Some(generator.expr(builder.get(replacement)))
    }

    fn method_expr(
        &self,
        builder: &mut Builder,
        method: &str,
        args: impl IntoIterator<Item = Expr<'a>>,
    ) -> BuildId<ruff_python_ast::kind::Expr> {
        let string = builder.copy_node(self.string);
        let args = builder.copy_nodes(args);
        syntax::call_method(builder, string, method, args)
    }
}
/// A literal that can be either a string or a bytes literal.
enum Literal<'a> {
    Str(ExprStringLiteral<'a>),
    Bytes(ExprBytesLiteral<'a>),
}

impl Literal<'_> {
    fn is_empty(&self) -> bool {
        match self {
            Literal::Str(str_lit) => str_lit.value().is_empty(),
            Literal::Bytes(bytes_lit) => bytes_lit.value().is_empty(),
        }
    }
}

/// Try to resolve `name` to either a string or bytes literal in `semantic`.
fn resolve_literal<'a>(name: Expr<'a>, semantic: &'a SemanticModel) -> Option<Literal<'a>> {
    if let Some(str_lit) = resolve_string_literal(name, semantic) {
        return Some(Literal::Str(str_lit));
    }
    if let Some(bytes_lit) = resolve_bytes_literal(name, semantic) {
        return Some(Literal::Bytes(bytes_lit));
    }
    None
}

/// Try to resolve `name` to an [`ExprBytesLiteral`] in `semantic`.
fn resolve_bytes_literal<'a>(
    name: Expr<'a>,
    semantic: &'a SemanticModel,
) -> Option<ExprBytesLiteral<'a>> {
    if name.is_bytes_literal_expr() {
        return name.as_bytes_literal_expr();
    }

    if let Some(name_expr) = name.as_name_expr() {
        let binding = semantic.binding(semantic.only_binding(name_expr)?);
        let value = find_binding_value(binding, semantic)?;
        if value.is_bytes_literal_expr() {
            return value.as_bytes_literal_expr();
        }
    }

    None
}

/// Try to resolve `name` to an [`ExprStringLiteral`] in `semantic`.
fn resolve_string_literal<'a>(
    name: Expr<'a>,
    semantic: &'a SemanticModel,
) -> Option<ExprStringLiteral<'a>> {
    if name.is_string_literal_expr() {
        return name.as_string_literal_expr();
    }

    if let Some(name_expr) = name.as_name_expr() {
        let binding = semantic.binding(semantic.only_binding(name_expr)?);
        let value = find_binding_value(binding, semantic)?;
        if value.is_string_literal_expr() {
            return value.as_string_literal_expr();
        }
    }

    None
}

#[derive(Clone, Copy, Debug)]
enum ComparisonToNone {
    Is,
    IsNot,
}

/// If the regex call is compared to `None`, return the comparison and its range.
///    Example: `re.search("abc", s) is None`
fn get_comparison_to_none(semantic: &SemanticModel) -> Option<(ComparisonToNone, TextRange)> {
    let compare = semantic.current_expression_parent()?.as_compare_expr()?;
    let (_, op, expr) = compare.as_single()?;
    let ExprKind::NoneLiteral(_) = expr.classify() else {
        return None;
    };

    match op {
        CmpOp::Is => Some((ComparisonToNone::Is, compare.range())),
        CmpOp::IsNot => Some((ComparisonToNone::IsNot, compare.range())),
        _ => None,
    }
}
