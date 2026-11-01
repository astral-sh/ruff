use ruff_python_ast::{Builder, StmtAssert, build};
use ruff_text_size::{Ranged, TextRange};

use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for uses of `assert expression, print(message)`.
///
/// ## Why is this bad?
/// If an `assert x, y` assertion fails, the Python interpreter raises an
/// `AssertionError`, and the evaluated value of `y` is used as the contents of
/// that assertion error. The `print` function always returns `None`, however,
/// so the evaluated value of a call to `print` will always be `None`.
///
/// Using a `print` call in this context will therefore output the message to
/// `stdout`, before raising an empty `AssertionError(None)`. Instead, remove
/// the `print` and pass the message directly as the second expression,
/// allowing `stderr` to capture the message in a well-formatted context.
///
/// ## Example
/// ```python
/// assert False, print("This is a message")
/// ```
///
/// Use instead:
/// ```python
/// assert False, "This is a message"
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as changing the second expression
/// will result in a different `AssertionError` message being raised, as well as
/// a change in `stdout` output.
///
/// ## References
/// - [Python documentation: `assert`](https://docs.python.org/3/reference/simple_stmts.html#the-assert-statement)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.8.0", category = Category::Correctness)]
pub(crate) struct AssertWithPrintMessage;

impl AlwaysFixableViolation for AssertWithPrintMessage {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`print()` call in `assert` statement is likely unintentional".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove `print`".to_owned()
    }
}

/// RUF030
///
/// Checks if the `msg` argument to an `assert` statement is a `print` call, and if so,
/// replace the message with the arguments to the `print` call.
pub(crate) fn assert_with_print_message(checker: &Checker, stmt: StmtAssert<'_>) {
    let Some(call) = stmt.msg().and_then(ruff_python_ast::Expr::as_call_expr) else {
        return;
    };
    if !checker.semantic().match_builtin_expr(call.func(), "print") {
        return;
    }
    let mut diagnostic = checker.report_diagnostic(AssertWithPrintMessage, call.range());
    let mut builder = Builder::default();
    let test = builder.copy_node(stmt.test());
    let msg = print_arguments::to_expr(&mut builder, call.arguments(), checker);
    let statement = builder.add(build::StmtAssert {
        range: TextRange::default(),
        test,
        msg,
    });
    // An empty print call removes the message and its leading comma.
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        checker.generator().stmt(builder.get(statement).into()),
        stmt.range(),
    )));
}
/// Builds an assertion message from the positional arguments and separator of a `print` call.
mod print_arguments {
    use ruff_python_ast::{
        Arguments, BuildId, Builder, Expr, InterpolatedStringElement, InterpolatedStringElementKind,
    };
    use ruff_text_size::TextRange;

    use crate::checkers::ast::Checker;
    use crate::fix::syntax;

    #[derive(Clone, Copy)]
    enum Part<'a> {
        Text(&'a str),
        Element(InterpolatedStringElement<'a>),
        Expression(Expr<'a>),
    }

    impl<'a> Part<'a> {
        fn text(self) -> Option<&'a str> {
            match self {
                Self::Text(text) => Some(text),
                Self::Element(element)
                    if let InterpolatedStringElementKind::Literal(literal) = element.classify() =>
                {
                    Some(literal.value())
                }
                _ => None,
            }
        }

        fn build(
            self,
            builder: &mut Builder,
        ) -> BuildId<ruff_python_ast::kind::InterpolatedStringElement> {
            match self {
                Self::Text(text) => syntax::literal_element(builder, text, TextRange::default()),
                Self::Element(element) => builder.copy_node(element),
                Self::Expression(expr) => syntax::interpolation(builder, expr),
            }
        }
    }

    fn append_parts<'a>(expr: Expr<'a>, parts: &mut Vec<Part<'a>>) {
        match expr.classify() {
            ruff_python_ast::ExprKind::StringLiteral(string) => {
                parts.extend(string.value().iter().map(|part| Part::Text(part.value())));
            }
            ruff_python_ast::ExprKind::FString(string) => {
                parts.extend(string.value().elements().map(Part::Element));
            }
            _ => parts.push(Part::Expression(expr)),
        }
    }

    pub(super) fn to_expr(
        builder: &mut Builder,
        arguments: Arguments<'_>,
        checker: &Checker,
    ) -> Option<BuildId<ruff_python_ast::kind::Expr>> {
        let mut args = arguments.args().iter();
        let first = args.next()?;
        let mut separator = Vec::new();
        if let Some(keyword) = arguments.find_keyword("sep")
            && !keyword.value().is_none_literal_expr()
        {
            append_parts(keyword.value(), &mut separator);
        } else {
            separator.push(Part::Text(" "));
        }
        // Insert the separator between print arguments. Adjacent literal parts within one argument stay together.
        let mut parts = Vec::new();
        append_parts(first, &mut parts);
        for arg in args {
            parts.extend_from_slice(&separator);
            append_parts(arg, &mut parts);
        }

        if separator.iter().all(|part| part.text().is_some())
            && let Some(text) = parts
                .iter()
                .copied()
                .map(Part::text)
                .collect::<Option<String>>()
        {
            return Some(syntax::string_literal(
                builder,
                &text,
                checker.default_string_flags(),
            ));
        }
        let elements = builder.map_list(parts, |builder, part| part.build(builder));
        Some(syntax::f_string(
            builder,
            elements,
            checker.default_fstring_flags(),
        ))
    }
}
