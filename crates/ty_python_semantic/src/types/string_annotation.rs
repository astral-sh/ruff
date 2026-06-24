use ruff_db::parsed::parsed_string_annotation;
use ruff_db::source::source_text;
use ruff_python_ast::{self as ast, ModExpression, StringFlags};
use ruff_python_parser::{ParseError, ParseErrorType, Parsed};
use ruff_text_size::Ranged;

use crate::declare_lint;
use crate::lint::{Level, LintStatus};
use crate::types::infer::InferenceFlags;

use super::context::InferContext;

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/raw-string-type-annotation.md")]
    pub(crate) static RAW_STRING_TYPE_ANNOTATION = {
        summary: "detects raw strings in type annotation positions",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/implicit-concatenated-string-type-annotation.md")]
    pub(crate) static IMPLICIT_CONCATENATED_STRING_TYPE_ANNOTATION = {
        summary: "detects implicit concatenated strings in type annotations",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-syntax-in-forward-annotation.md")]
    pub(crate) static INVALID_SYNTAX_IN_FORWARD_ANNOTATION = {
        summary: "detects invalid syntax in forward annotations",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/escape-character-in-forward-annotation.md")]
    pub(crate) static ESCAPE_CHARACTER_IN_FORWARD_ANNOTATION = {
        summary: "detects forward type annotations with escape characters",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

/// Parses the given expression as a string annotation.
pub(crate) fn parse_string_annotation(
    context: &InferContext,
    inference_flags: InferenceFlags,
    string_expr: &ast::ExprStringLiteral,
) -> Option<Parsed<ModExpression>> {
    let file = context.file();
    let db = context.db();

    let _span = tracing::trace_span!("parse_string_annotation", string=?string_expr.range(), ?file)
        .entered();

    let source = source_text(db, file);

    if let Some(string_literal) = string_expr.as_single_part_string() {
        let prefix = string_literal.flags.prefix();
        if prefix.is_raw() {
            if let Some(builder) = context.report_lint(&RAW_STRING_TYPE_ANNOTATION, string_literal)
            {
                builder.into_diagnostic(format_args!(
                    "Raw string literals are not allowed in {}s",
                    inference_flags.type_expression_context()
                ));
            }
        // Compare the raw contents (without quotes) of the expression with the parsed contents
        // contained in the string literal.
        } else if &source[string_literal.content_range()] == string_literal.as_str() {
            match parsed_string_annotation(source.as_str(), string_literal) {
                Ok(parsed) => return Some(parsed),
                Err(ParseError { error, location }) => {
                    if let Some(builder) =
                        context.report_lint(&INVALID_SYNTAX_IN_FORWARD_ANNOTATION, location)
                    {
                        let mut diagnostic =
                            builder.into_diagnostic("Syntax error in forward annotation");

                        diagnostic.set_primary_message(&error);

                        let possible_secondary = string_literal
                            .range()
                            .add_start(string_literal.flags.opener_len())
                            .sub_end(string_literal.flags.closer_len());
                        if possible_secondary.contains_range(location)
                            && (possible_secondary.start() < location.start()
                                || possible_secondary.end() > location.end())
                        {
                            diagnostic.annotate(context.secondary(possible_secondary));
                        }

                        if !matches!(error, ParseErrorType::StringAnnotationError(_))
                            && !string_literal.contains('\n')
                        {
                            diagnostic.help(format_args!(
                                "Did you mean `typing.Literal[\"{}\"]`?",
                                string_literal.as_str()
                            ));
                        }
                    }
                }
            }
        } else if let Some(builder) =
            context.report_lint(&ESCAPE_CHARACTER_IN_FORWARD_ANNOTATION, string_expr)
        {
            // The raw contents of the string doesn't match the parsed content. This could be the
            // case for annotations that contain escape sequences.
            builder.into_diagnostic(format_args!(
                "Escape characters are not allowed in {}s",
                inference_flags.type_expression_context()
            ));
        }
    } else if let Some(builder) =
        context.report_lint(&IMPLICIT_CONCATENATED_STRING_TYPE_ANNOTATION, string_expr)
    {
        // String is implicitly concatenated.
        builder.into_diagnostic("Type expressions cannot span multiple string literals");
    }

    None
}
