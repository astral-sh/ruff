use ruff_db::files::File;
use ruff_db::source::source_text;
use ruff_python_ast::str::raw_contents;
use ruff_python_ast::{self as ast, ModExpression, StringFlags};
use ruff_python_parser::{parse_expression_range, Parsed};
use ruff_text_size::Ranged;

use crate::lint::{Level, LintStatus};
use crate::types::diagnostic::{TypeCheckDiagnostics, TypeCheckDiagnosticsBuilder};
use crate::{declare_lint, Db};

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

type AnnotationParseResult = Result<Parsed<ModExpression>, TypeCheckDiagnostics>;

/// Parses the given expression as a string annotation.
pub(crate) fn parse_string_annotation(
    db: &dyn Db,
    file: File,
    string_expr: &ast::ExprStringLiteral,
) -> AnnotationParseResult {
    let _span = tracing::trace_span!("parse_string_annotation", string=?string_expr.range(), file=%file.path(db)).entered();

    let source = source_text(db.upcast(), file);
    let node_text = &source[string_expr.range()];
    let mut diagnostics = TypeCheckDiagnosticsBuilder::new(db, file);

    if let [string_literal] = string_expr.value.as_slice() {
        let prefix = string_literal.flags.prefix();
        if prefix.is_raw() {
            diagnostics.add_lint(
                &RAW_STRING_TYPE_ANNOTATION,
                string_literal.into(),
                format_args!("Type expressions cannot use raw string literal"),
            );
        // Compare the raw contents (without quotes) of the expression with the parsed contents
        // contained in the string literal.
        } else if raw_contents(node_text)
            .is_some_and(|raw_contents| raw_contents == string_literal.as_str())
        {
            let range_excluding_quotes = string_literal
                .range()
                .add_start(string_literal.flags.opener_len())
                .sub_end(string_literal.flags.closer_len());

            // TODO: Support multiline strings like:
            // ```py
            // x: """
            //     int
            //     | float
            // """ = 1
            // ```
            match parse_expression_range(source.as_str(), range_excluding_quotes) {
                Ok(parsed) => return Ok(parsed),
                Err(parse_error) => diagnostics.add_lint(
                    &INVALID_SYNTAX_IN_FORWARD_ANNOTATION,
                    string_literal.into(),
                    format_args!("Syntax error in forward annotation: {}", parse_error.error),
                ),
            }
        } else {
            // The raw contents of the string doesn't match the parsed content. This could be the
            // case for annotations that contain escape sequences.
            diagnostics.add_lint(
                &ESCAPE_CHARACTER_IN_FORWARD_ANNOTATION,
                string_expr.into(),
                format_args!("Type expressions cannot contain escape characters"),
            );
        }
    } else {
        // String is implicitly concatenated.
        diagnostics.add_lint(
            &IMPLICIT_CONCATENATED_STRING_TYPE_ANNOTATION,
            string_expr.into(),
            format_args!("Type expressions cannot span multiple string literals"),
        );
    }

    Err(diagnostics.finish())
}
