use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    Arguments, CmpOp, Expr, ExprAttribute, ExprCall, ExprCompare, ExprContext, Identifier,
};
use ruff_python_semantic::Modules;
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

/// ## What it does
///
/// Reports the following `re` calls when their first arguments are plain string
/// literals, and no additional flags are passed:
///
/// - `sub`
/// - `match`
/// - `search`
/// - `fullmatch`
/// - `split`
///
/// For `sub`, the `repl` (replacement) argument must also be a string literal,
/// not a function. For `match`, `search`, and `fullmatch`, the return value
/// must also be used only for its truth value.
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
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryRegularExpression {
    replacement: String,
}

impl AlwaysFixableViolation for UnnecessaryRegularExpression {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Plain string pattern passed to `re` function".to_string()
    }

    fn fix_title(&self) -> String {
        format!("Replace with `{}`", self.replacement)
    }
}

/// The `re` functions supported by this rule.
#[derive(Debug)]
enum ReFuncKind<'a> {
    Sub { repl: &'a Expr },
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
        checker: &'a mut Checker,
        call: &'a ExprCall,
        func_name: &str,
    ) -> Option<Self> {
        let nargs = call.arguments.len();
        let locate_arg = |name, position| call.arguments.find_argument(name, position);

        // the proposed fixes for match, search, and fullmatch rely on the
        // return value only being used for its truth value
        let in_if_context = checker.semantic().in_boolean_test();

        match (func_name, nargs) {
            // `split` is the safest of these to fix, as long as metacharacters
            // have already been filtered out from the `pattern`
            ("split", 2) => Some(ReFunc {
                kind: ReFuncKind::Split,
                pattern: locate_arg("pattern", 0)?,
                string: locate_arg("string", 1)?,
            }),
            // `sub` is only safe to fix if `repl` is a string. `re.sub` also
            // allows it to be a function, which will *not* work in the str
            // version
            ("sub", 3) => {
                let repl = call.arguments.find_argument("repl", 1)?;
                if !repl.is_string_literal_expr() {
                    return None;
                }
                Some(ReFunc {
                    kind: ReFuncKind::Sub { repl },
                    pattern: locate_arg("pattern", 0)?,
                    string: locate_arg("string", 2)?,
                })
            }
            ("match", 2) if in_if_context => Some(ReFunc {
                kind: ReFuncKind::Match,
                pattern: locate_arg("pattern", 0)?,
                string: locate_arg("string", 1)?,
            }),
            ("search", 2) if in_if_context => Some(ReFunc {
                kind: ReFuncKind::Search,
                pattern: locate_arg("pattern", 0)?,
                string: locate_arg("string", 1)?,
            }),
            ("fullmatch", 2) if in_if_context => Some(ReFunc {
                kind: ReFuncKind::Fullmatch,
                pattern: locate_arg("pattern", 0)?,
                string: locate_arg("string", 1)?,
            }),
            _ => None,
        }
    }

    fn replacement(&self) -> Expr {
        match self.kind {
            // string.replace(pattern, repl)
            ReFuncKind::Sub { repl } => {
                self.method_expr("replace", vec![self.pattern.clone(), repl.clone()])
            }
            // string.startswith(pattern)
            ReFuncKind::Match => self.method_expr("startswith", vec![self.pattern.clone()]),
            // pattern in string
            ReFuncKind::Search => self.compare_expr(CmpOp::In),
            // string == pattern
            ReFuncKind::Fullmatch => self.compare_expr(CmpOp::Eq),
            // string.split(pattern)
            ReFuncKind::Split => self.method_expr("split", vec![self.pattern.clone()]),
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

/// RUF055
pub(crate) fn unnecessary_regular_expression(checker: &mut Checker, call: &ExprCall) {
    // adapted from unraw_re_pattern
    let semantic = checker.semantic();

    if !semantic.seen_module(Modules::RE) {
        return;
    }

    let Some(qualified_name) = semantic.resolve_qualified_name(call.func.as_ref()) else {
        return;
    };

    let ["re", func] = qualified_name.segments() else {
        return;
    };

    // skip calls with more than `pattern` and `string` arguments (and `repl`
    // for `sub`)
    let Some(re_func) = ReFunc::from_call_expr(checker, call, func) else {
        return;
    };

    let Some(pat) = call.arguments.find_argument("pattern", 0) else {
        // this should be unreachable given the checks above, so it might be
        // safe to unwrap here instead
        return;
    };

    // For now, restrict this rule to string literals
    let Some(string_lit) = pat.as_string_literal_expr() else {
        return;
    };

    // For now, reject any regex metacharacters. Compare to the complete list
    // from https://docs.python.org/3/howto/regex.html#matching-characters
    let is_plain_string = !string_lit.value.chars().any(|c| {
        matches!(
            c,
            '.' | '^' | '$' | '*' | '+' | '?' | '{' | '}' | '[' | ']' | '\\' | '|' | '(' | ')'
        )
    });

    if !is_plain_string {
        return;
    }

    // Here we know the pattern is a string literal with no metacharacters, so
    // we can proceed with the str method replacement
    let new_expr = re_func.replacement();

    let repl = checker.generator().expr(&new_expr);
    let mut diagnostic = Diagnostic::new(
        UnnecessaryRegularExpression {
            replacement: repl.clone(),
        },
        call.range,
    );

    diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
        repl,
        call.range.start(),
        call.range.end(),
    )));

    checker.diagnostics.push(diagnostic);
}
