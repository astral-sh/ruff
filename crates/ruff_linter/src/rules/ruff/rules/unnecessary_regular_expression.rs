use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::ExprCall;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

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
    Sub { repl: &'a str },
    Match,
    Search,
    Fullmatch,
    Split,
}

#[derive(Debug)]
struct ReFunc<'a> {
    kind: ReFuncKind<'a>,
    pattern: &'a str,
    string: &'a str,
}

impl<'a> ReFunc<'a> {
    fn from_call_expr(checker: &'a mut Checker, call: &ExprCall, func_name: &str) -> Option<Self> {
        let locator = checker.locator();
        let nargs = call.arguments.len();
        let locate_arg = |name, position| {
            Some(locator.slice(call.arguments.find_argument(name, position)?.range()))
        };
        match (func_name, nargs) {
            ("sub", 3) => Some(ReFunc {
                kind: ReFuncKind::Sub {
                    repl: locate_arg("repl", 1)?,
                },
                pattern: locate_arg("pattern", 0)?,
                string: locate_arg("string", 2)?,
            }),
            ("match", 2) => Some(ReFunc {
                kind: ReFuncKind::Match,
                pattern: locate_arg("pattern", 0)?,
                string: locate_arg("string", 1)?,
            }),
            ("search", 2) => Some(ReFunc {
                kind: ReFuncKind::Search,
                pattern: locate_arg("pattern", 0)?,
                string: locate_arg("string", 1)?,
            }),
            ("fullmatch", 2) => Some(ReFunc {
                kind: ReFuncKind::Fullmatch,
                pattern: locate_arg("pattern", 0)?,
                string: locate_arg("string", 1)?,
            }),
            ("split", 2) => Some(ReFunc {
                kind: ReFuncKind::Split,
                pattern: locate_arg("pattern", 0)?,
                string: locate_arg("string", 1)?,
            }),
            _ => None,
        }
    }

    fn replacement(&self) -> String {
        let Self {
            kind,
            pattern,
            string,
        } = &self;
        match kind {
            ReFuncKind::Sub { repl } => {
                format!("{string}.replace({pattern}, {repl})")
            }
            ReFuncKind::Match => format!("{string}.startswith({pattern})"),
            ReFuncKind::Search => format!("{pattern} in {string}"),
            ReFuncKind::Fullmatch => format!("{pattern} == {string}"),
            ReFuncKind::Split => format!("{string}.split({pattern})"),
        }
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
    let repl = re_func.replacement();

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
