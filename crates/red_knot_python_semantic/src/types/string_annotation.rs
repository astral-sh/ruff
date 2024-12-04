use ruff_db::files::File;
use ruff_db::source::source_text;
use ruff_python_ast::str::raw_contents;
use ruff_python_ast::{self as ast, ModExpression, StringFlags};
use ruff_python_parser::{parse_expression_range, Parsed};
use ruff_text_size::Ranged;

use crate::types::diagnostic::report_type_diagnostic;
use crate::Db;

type AnnotationParseResult = Result<Parsed<ModExpression>, ()>;

/// Parses the given expression as a string annotation.
pub(crate) fn parse_string_annotation(
    db: &dyn Db,
    file: File,
    string_expr: &ast::ExprStringLiteral,
) -> AnnotationParseResult {
    let _span = tracing::trace_span!("parse_string_annotation", string=?string_expr.range(), file=%file.path(db)).entered();

    let source = source_text(db.upcast(), file);
    let node_text = &source[string_expr.range()];

    if let [string_literal] = string_expr.value.as_slice() {
        let prefix = string_literal.flags.prefix();
        if prefix.is_raw() {
            report_type_diagnostic(
                db,
                file,
                string_literal.into(),
                "annotation-raw-string",
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
                Err(parse_error) => report_type_diagnostic(
                    db,
                    file,
                    string_literal.into(),
                    "forward-annotation-syntax-error",
                    format_args!("Syntax error in forward annotation: {}", parse_error.error),
                ),
            }
        } else {
            // The raw contents of the string doesn't match the parsed content. This could be the
            // case for annotations that contain escape sequences.
            report_type_diagnostic(
                db,
                file,
                string_expr.into(),
                "annotation-escape-character",
                format_args!("Type expressions cannot contain escape characters"),
            );
        }
    } else {
        // String is implicitly concatenated.
        report_type_diagnostic(
            db,
            file,
            string_expr.into(),
            "annotation-implicit-concat",
            format_args!("Type expressions cannot span multiple string literals"),
        );
    }

    Err(())
}
