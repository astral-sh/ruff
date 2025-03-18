use ruff_db::source::source_text;
use ruff_python_ast::{self as ast, ModExpression};
use ruff_python_parser::Parsed;
use ruff_text_size::Ranged;

use crate::declare_lint;
use crate::lint::{Level, LintStatus};

use super::context::InferContext;

declare_lint! {
    /// ## What it does
    /// Checks for f-strings in type annotation positions.
    ///
    /// ## Why is this bad?
    /// Static analysis tools like Red Knot can't analyse type annotations that use f-string notation.
    ///
    /// ## Examples
    /// ```python
    /// def test(): -> f"int":
    ///     ...
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// def test(): -> "int":
    ///     ...
    /// ```
    pub(crate) static FSTRING_TYPE_ANNOTATION = {
        summary: "detects F-strings in type annotation positions",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
        /// ## What it does
        /// Checks for byte-strings in type annotation positions.
        ///
        /// ## Why is this bad?
        /// Static analysis tools like Red Knot can't analyse type annotations that use byte-string notation.
        ///
        /// ## Examples
        /// ```python
        /// def test(): -> b"int":
        ///     ...
        /// ```
        ///
        /// Use instead:
        /// ```python
        /// def test(): -> "int":
        ///     ...
        /// ```
    pub(crate) static BYTE_STRING_TYPE_ANNOTATION = {
        summary: "detects byte strings in type annotation positions",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
        /// ## What it does
        /// Checks for raw-strings in type annotation positions.
        ///
        /// ## Why is this bad?
        /// Static analysis tools like Red Knot can't analyse type annotations that use raw-string notation.
        ///
        /// ## Examples
        /// ```python
        /// def test(): -> r"int":
        ///     ...
        /// ```
        ///
        /// Use instead:
        /// ```python
        /// def test(): -> "int":
        ///     ...
        /// ```
    pub(crate) static RAW_STRING_TYPE_ANNOTATION = {
        summary: "detects raw strings in type annotation positions",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
        /// ## What it does
        /// Checks for implicit concatenated strings in type annotation positions.
        ///
        /// ## Why is this bad?
        /// Static analysis tools like Red Knot can't analyse type annotations that use implicit concatenated strings.
        ///
        /// ## Examples
        /// ```python
        /// def test(): -> "Literal[" "5" "]":
        ///     ...
        /// ```
        ///
        /// Use instead:
        /// ```python
        /// def test(): -> "Literal[5]":
        ///     ...
        /// ```
    pub(crate) static IMPLICIT_CONCATENATED_STRING_TYPE_ANNOTATION = {
        summary: "detects implicit concatenated strings in type annotations",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static INVALID_SYNTAX_IN_FORWARD_ANNOTATION = {
        summary: "detects invalid syntax in forward annotations",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static ESCAPE_CHARACTER_IN_FORWARD_ANNOTATION = {
        summary: "detects forward type annotations with escape characters",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

/// Parses the given expression as a string annotation.
pub(crate) fn parse_string_annotation(
    context: &InferContext,
    string_expr: &ast::ExprStringLiteral,
) -> Option<Parsed<ModExpression>> {
    let file = context.file();
    let db = context.db();

    let _span = tracing::trace_span!("parse_string_annotation", string=?string_expr.range(), ?file)
        .entered();

    let source = source_text(db.upcast(), file);

    if let Some(string_literal) = string_expr.as_single_part_string() {
        let prefix = string_literal.flags.prefix();
        if prefix.is_raw() {
            context.report_lint(
                &RAW_STRING_TYPE_ANNOTATION,
                string_literal,
                format_args!("Type expressions cannot use raw string literal"),
            );
        // Compare the raw contents (without quotes) of the expression with the parsed contents
        // contained in the string literal.
        } else if &source[string_literal.content_range()] == string_literal.as_str() {
            match ruff_python_parser::parse_string_annotation(source.as_str(), string_literal) {
                Ok(parsed) => return Some(parsed),
                Err(parse_error) => context.report_lint(
                    &INVALID_SYNTAX_IN_FORWARD_ANNOTATION,
                    string_literal,
                    format_args!("Syntax error in forward annotation: {}", parse_error.error),
                ),
            }
        } else {
            // The raw contents of the string doesn't match the parsed content. This could be the
            // case for annotations that contain escape sequences.
            context.report_lint(
                &ESCAPE_CHARACTER_IN_FORWARD_ANNOTATION,
                string_expr,
                format_args!("Type expressions cannot contain escape characters"),
            );
        }
    } else {
        // String is implicitly concatenated.
        context.report_lint(
            &IMPLICIT_CONCATENATED_STRING_TYPE_ANNOTATION,
            string_expr,
            format_args!("Type expressions cannot span multiple string literals"),
        );
    }

    None
}
