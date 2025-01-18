use itertools::Itertools;
use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    Arguments, CmpOp, Expr, ExprAttribute, ExprCall, ExprCompare, ExprContext, ExprStringLiteral,
    Identifier,
};
use ruff_python_semantic::analyze::typing::find_binding_value;
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

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
/// - `re.search`
/// - `re.fullmatch`
/// - `re.split`
///
/// For `re.sub`, the `repl` (replacement) argument must also be a string literal,
/// not a function. For `re.match`, `re.search`, and `re.fullmatch`, the return
/// value must also be used only for its truth value.
///
/// ## Fix safety
///
/// This rule's fix is marked as unsafe if the affected expression contains comments. Otherwise,
/// the fix can be applied safely.
///
/// ## References
/// - [Python Regular Expression HOWTO: Common Problems - Use String Methods](https://docs.python.org/3/howto/regex.html#use-string-methods)
#[derive(ViolationMetadata)]
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

/// RUF055
pub(crate) fn unnecessary_regular_expression(checker: &mut Checker, call: &ExprCall) {
    // adapted from unraw_re_pattern
    let semantic = checker.semantic();

    if !semantic.seen_module(Modules::RE) {
        return;
    }

    let Some(qualified_name) = semantic.resolve_qualified_name(&call.func) else {
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
    let Some(string_lit) = resolve_string_literal(re_func.pattern, semantic) else {
        return;
    };

    // For now, reject any regex metacharacters. Compare to the complete list
    // from https://docs.python.org/3/howto/regex.html#matching-characters
    let has_metacharacters = string_lit
        .value
        .to_str()
        .contains(['.', '^', '$', '*', '+', '?', '{', '[', '\\', '|', '(', ')']);

    if has_metacharacters {
        return;
    }

    // Here we know the pattern is a string literal with no metacharacters, so
    // we can proceed with the str method replacement
    let new_expr = re_func.replacement();

    let repl = new_expr.map(|expr| checker.generator().expr(&expr));
    let mut diagnostic = Diagnostic::new(
        UnnecessaryRegularExpression {
            replacement: repl.clone(),
        },
        call.range,
    );

    if let Some(repl) = repl {
        diagnostic.set_fix(Fix::applicable_edit(
            Edit::range_replacement(repl, call.range),
            if checker
                .comment_ranges()
                .has_comments(call, checker.source())
            {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            },
        ));
    }

    checker.diagnostics.push(diagnostic);
}

/// The `re` functions supported by this rule.
#[derive(Debug)]
enum ReFuncKind<'a> {
    // Only `Some` if it's a fixable `re.sub()` call
    Sub { repl: Option<&'a Expr> },
    Match,
    Search,
    Fullmatch,
    Split,
}

#[derive(Debug)]
struct ReFunc<'a> {
    kind: ReFuncKind<'a>,
    pattern: &'a Expr,
    string: &'a Expr,
}

impl<'a> ReFunc<'a> {
    fn from_call_expr(
        semantic: &'a SemanticModel,
        call: &'a ExprCall,
        func_name: &str,
    ) -> Option<Self> {
        // the proposed fixes for match, search, and fullmatch rely on the
        // return value only being used for its truth value
        let in_if_context = semantic.in_boolean_test();

        match (func_name, call.arguments.len()) {
            // `split` is the safest of these to fix, as long as metacharacters
            // have already been filtered out from the `pattern`
            ("split", 2) => Some(ReFunc {
                kind: ReFuncKind::Split,
                pattern: call.arguments.find_argument_value("pattern", 0)?,
                string: call.arguments.find_argument_value("string", 1)?,
            }),
            // `sub` is only safe to fix if `repl` is a string. `re.sub` also
            // allows it to be a function, which will *not* work in the str
            // version
            ("sub", 3) => {
                let repl = call.arguments.find_argument_value("repl", 1)?;
                let lit = resolve_string_literal(repl, semantic)?;
                let mut fixable = true;
                for (c, next) in lit.value.chars().tuple_windows() {
                    // `\0` (or any other ASCII digit) and `\g` have special meaning in `repl` strings.
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
                        if "abfnrtv".contains(next) {
                            fixable = false;
                        } else {
                            return None;
                        }
                    }
                }
                Some(ReFunc {
                    kind: ReFuncKind::Sub {
                        repl: fixable.then_some(repl),
                    },
                    pattern: call.arguments.find_argument_value("pattern", 0)?,
                    string: call.arguments.find_argument_value("string", 2)?,
                })
            }
            ("match", 2) if in_if_context => Some(ReFunc {
                kind: ReFuncKind::Match,
                pattern: call.arguments.find_argument_value("pattern", 0)?,
                string: call.arguments.find_argument_value("string", 1)?,
            }),
            ("search", 2) if in_if_context => Some(ReFunc {
                kind: ReFuncKind::Search,
                pattern: call.arguments.find_argument_value("pattern", 0)?,
                string: call.arguments.find_argument_value("string", 1)?,
            }),
            ("fullmatch", 2) if in_if_context => Some(ReFunc {
                kind: ReFuncKind::Fullmatch,
                pattern: call.arguments.find_argument_value("pattern", 0)?,
                string: call.arguments.find_argument_value("string", 1)?,
            }),
            _ => None,
        }
    }

    fn replacement(&self) -> Option<Expr> {
        match self.kind {
            // string.replace(pattern, repl)
            ReFuncKind::Sub { repl } => repl
                .cloned()
                .map(|repl| self.method_expr("replace", vec![self.pattern.clone(), repl])),
            // string.startswith(pattern)
            ReFuncKind::Match => Some(self.method_expr("startswith", vec![self.pattern.clone()])),
            // pattern in string
            ReFuncKind::Search => Some(self.compare_expr(CmpOp::In)),
            // string == pattern
            ReFuncKind::Fullmatch => Some(self.compare_expr(CmpOp::Eq)),
            // string.split(pattern)
            ReFuncKind::Split => Some(self.method_expr("split", vec![self.pattern.clone()])),
        }
    }

    /// Return a new compare expr of the form `self.pattern op self.string`
    fn compare_expr(&self, op: CmpOp) -> Expr {
        Expr::Compare(ExprCompare {
            left: Box::new(self.pattern.clone()),
            ops: Box::new([op]),
            comparators: Box::new([self.string.clone()]),
            range: TextRange::default(),
        })
    }

    /// Return a new method call expression on `self.string` with `args` like
    /// `self.string.method(args...)`
    fn method_expr(&self, method: &str, args: Vec<Expr>) -> Expr {
        let method = Expr::Attribute(ExprAttribute {
            value: Box::new(self.string.clone()),
            attr: Identifier::new(method, TextRange::default()),
            ctx: ExprContext::Load,
            range: TextRange::default(),
        });
        Expr::Call(ExprCall {
            func: Box::new(method),
            arguments: Arguments {
                args: args.into_boxed_slice(),
                keywords: Box::new([]),
                range: TextRange::default(),
            },
            range: TextRange::default(),
        })
    }
}

/// Try to resolve `name` to an [`ExprStringLiteral`] in `semantic`.
fn resolve_string_literal<'a>(
    name: &'a Expr,
    semantic: &'a SemanticModel,
) -> Option<&'a ExprStringLiteral> {
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
