use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};

use crate::checkers::ast::Checker;

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
pub(crate) fn assert_with_print_message(checker: &Checker, stmt: &ast::StmtAssert) {
    if let Some(Expr::Call(call)) = stmt.msg.as_deref() {
        // We have to check that the print call is a call to the built-in `print` function
        let semantic = checker.semantic();

        if semantic.match_builtin_expr(&call.func, "print") {
            // This is the confirmed rule condition
            let mut diagnostic = Diagnostic::new(AssertWithPrintMessage, call.range());
            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                checker.generator().stmt(&Stmt::Assert(ast::StmtAssert {
                    test: stmt.test.clone(),
                    msg: print_arguments::to_expr(&call.arguments, checker).map(Box::new),
                    range: TextRange::default(),
                })),
                // We have to replace the entire statement,
                // as the `print` could be empty and thus `call.range()`
                // will cease to exist.
                stmt.range(),
            )));
            checker.report_diagnostic(diagnostic);
        }
    }
}

/// Extracts the arguments from a `print` call and converts them to some kind of string
/// expression.
///
/// Three cases are handled:
/// - if there are no arguments, return `None` so that `diagnostic` can remove `msg` from `assert`;
/// - if all of `print` arguments including `sep` are string literals, return a `Expr::StringLiteral`;
/// - otherwise, return a `Expr::FString`.
mod print_arguments {
    use itertools::Itertools;
    use ruff_python_ast::{
        Arguments, ConversionFlag, Expr, ExprFString, FString, FStringElement, FStringElements,
        FStringExpressionElement, FStringFlags, FStringLiteralElement, FStringValue, StringLiteral,
        StringLiteralFlags,
    };
    use ruff_text_size::TextRange;

    use crate::checkers::ast::Checker;

    /// Converts an expression to a list of `FStringElement`s.
    ///
    /// Three cases are handled:
    /// - if the expression is a string literal, each part of the string will be converted to a
    ///   `FStringLiteralElement`.
    /// - if the expression is an f-string, the elements will be returned as-is.
    /// - otherwise, the expression will be wrapped in a `FStringExpressionElement`.
    fn expr_to_fstring_elements(expr: &Expr) -> Vec<FStringElement> {
        match expr {
            // If the expression is a string literal, convert each part to a `FStringLiteralElement`.
            Expr::StringLiteral(string) => string
                .value
                .iter()
                .map(|part| {
                    FStringElement::Literal(FStringLiteralElement {
                        value: part.value.clone(),
                        range: TextRange::default(),
                    })
                })
                .collect(),

            // If the expression is an f-string, return the elements.
            Expr::FString(fstring) => fstring.value.elements().cloned().collect(),

            // Otherwise, return the expression as a single `FStringExpressionElement` wrapping
            // the expression.
            expr => vec![FStringElement::Expression(FStringExpressionElement {
                expression: Box::new(expr.clone()),
                debug_text: None,
                conversion: ConversionFlag::None,
                format_spec: None,
                range: TextRange::default(),
            })],
        }
    }

    /// Converts a list of `FStringElement`s to a list of `StringLiteral`s.
    ///
    /// If any of the elements are not string literals, `None` is returned.
    ///
    /// This is useful (in combination with [`expr_to_fstring_elements`]) for
    /// checking if the `sep` and `args` arguments to `print` are all string
    /// literals.
    fn fstring_elements_to_string_literals<'a>(
        mut elements: impl ExactSizeIterator<Item = &'a FStringElement>,
        flags: StringLiteralFlags,
    ) -> Option<Vec<StringLiteral>> {
        elements.try_fold(Vec::with_capacity(elements.len()), |mut acc, element| {
            if let FStringElement::Literal(literal) = element {
                acc.push(StringLiteral {
                    value: literal.value.clone(),
                    flags,
                    range: TextRange::default(),
                });
                Some(acc)
            } else {
                None
            }
        })
    }

    /// Converts the `sep` and `args` arguments to a [`Expr::StringLiteral`].
    ///
    /// This function will return [`None`] if any of the arguments are not string literals,
    /// or if there are no arguments at all.
    fn args_to_string_literal_expr<'a>(
        args: impl ExactSizeIterator<Item = &'a Vec<FStringElement>>,
        sep: impl ExactSizeIterator<Item = &'a FStringElement>,
        flags: StringLiteralFlags,
    ) -> Option<Expr> {
        // If there are no arguments, short-circuit and return `None`
        if args.len() == 0 {
            return None;
        }

        // Attempt to convert the `sep` and `args` arguments to string literals.
        // We need to maintain `args` as a Vec of Vecs, as the first Vec represents
        // the arguments to the `print` call, and the inner Vecs represent the elements
        // of a concatenated string literal. (e.g. "text", "text" "text") The `sep` will
        // be inserted only between the outer Vecs.
        let (Some(sep), Some(args)) = (
            fstring_elements_to_string_literals(sep, flags),
            args.map(|arg| fstring_elements_to_string_literals(arg.iter(), flags))
                .collect::<Option<Vec<_>>>(),
        ) else {
            // If any of the arguments are not string literals, return None
            return None;
        };

        // Put the `sep` into a single Rust `String`
        let sep_string = sep
            .into_iter()
            .map(|string_literal| string_literal.value)
            .join("");

        // Join the `args` with the `sep`
        let combined_string = args
            .into_iter()
            .map(|string_literals| {
                string_literals
                    .into_iter()
                    .map(|string_literal| string_literal.value)
                    .join("")
            })
            .join(&sep_string);

        Some(Expr::from(StringLiteral {
            value: combined_string.into(),
            flags,
            range: TextRange::default(),
        }))
    }

    /// Converts the `sep` and `args` arguments to a [`Expr::FString`].
    ///
    /// This function will only return [`None`] if there are no arguments at all.
    ///
    /// ## Note
    /// This function will always return an f-string, even if all arguments are string literals.
    /// This can produce unnecessary f-strings.
    ///
    /// Also note that the iterator arguments of this function are consumed,
    /// as opposed to the references taken by [`args_to_string_literal_expr`].
    fn args_to_fstring_expr(
        mut args: impl ExactSizeIterator<Item = Vec<FStringElement>>,
        sep: impl ExactSizeIterator<Item = FStringElement>,
        flags: FStringFlags,
    ) -> Option<Expr> {
        // If there are no arguments, short-circuit and return `None`
        let first_arg = args.next()?;
        let sep = sep.collect::<Vec<_>>();

        let fstring_elements = args.fold(first_arg, |mut elements, arg| {
            elements.extend(sep.clone());
            elements.extend(arg);
            elements
        });

        Some(Expr::FString(ExprFString {
            value: FStringValue::single(FString {
                elements: FStringElements::from(fstring_elements),
                flags,
                range: TextRange::default(),
            }),
            range: TextRange::default(),
        }))
    }

    /// Attempts to convert the `print` arguments to a suitable string expression.
    ///
    /// If the `sep` argument is provided, it will be used as the separator between
    /// arguments. Otherwise, a space will be used.
    ///
    /// `end` and `file` keyword arguments are ignored, as they don't affect the
    /// output of the `print` statement.
    ///
    /// ## Returns
    ///
    /// - [`Some`]<[`Expr::StringLiteral`]> if all arguments including `sep` are string literals.
    /// - [`Some`]<[`Expr::FString`]> if any of the arguments are not string literals.
    /// - [`None`] if the `print` contains no positional arguments at all.
    pub(super) fn to_expr(arguments: &Arguments, checker: &Checker) -> Option<Expr> {
        // Convert the `sep` argument into `FStringElement`s
        let sep = arguments
            .find_keyword("sep")
            .and_then(
                // If the `sep` argument is `None`, treat this as default behavior.
                |keyword| {
                    if let Expr::NoneLiteral(_) = keyword.value {
                        None
                    } else {
                        Some(&keyword.value)
                    }
                },
            )
            .map(expr_to_fstring_elements)
            .unwrap_or_else(|| {
                vec![FStringElement::Literal(FStringLiteralElement {
                    range: TextRange::default(),
                    value: " ".into(),
                })]
            });

        let args = arguments
            .args
            .iter()
            .map(expr_to_fstring_elements)
            .collect::<Vec<_>>();

        // Attempt to convert the `sep` and `args` arguments to a string literal,
        // falling back to an f-string if the arguments are not all string literals.
        args_to_string_literal_expr(args.iter(), sep.iter(), checker.default_string_flags())
            .or_else(|| {
                args_to_fstring_expr(
                    args.into_iter(),
                    sep.into_iter(),
                    checker.default_fstring_flags(),
                )
            })
    }
}
