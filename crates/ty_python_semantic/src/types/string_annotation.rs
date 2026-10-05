use std::convert::Infallible;

use ruff_db::parsed::parsed_string_annotation;
use ruff_db::source::{SourceText, source_text};
use ruff_python_ast::{self as ast, ModExpression, StringFlags};
use ruff_python_parser::{ParseError, ParseErrorType, Parsed};
use ruff_text_size::Ranged;

use crate::declare_lint;
use crate::lint::{Level, LintStatus};
use crate::types::diagnostic::autofix_with_literal;
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

    let _span = tracing::trace_span!("parse_string_annotation", string=?string_expr.range(), ?file)
        .entered();

    match parse_string_annotation_sync(
        context,
        inference_flags,
        string_expr,
        &OrdinaryStringAnnotationEffects,
    ) {
        Ok(parsed) => parsed,
        Err(never) => match never {},
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    /// Supplies source, parsing, and diagnostics for quoted type expressions.
    #[synchronous(SynchronousStringAnnotationEffects)]
    pub(in crate::types) trait StringAnnotationEffects<'db, 'ast> {
        type Error;

        /// Reads the source containing the string, including before a validation rejection.
        #[operation(source)]
        async fn source(&self, context: &InferContext<'db, 'ast>) -> Result<SourceText, Self::Error>;

        /// Selects the only literal when the expression is not implicitly concatenated.
        #[operation(local)]
        async fn single_part<'string>(
            &self,
            string_expr: &'string ast::ExprStringLiteral,
        ) -> Result<Option<&'string ast::StringLiteral>, Self::Error>;

        /// Checks whether the literal has a raw-string prefix.
        #[operation(local)]
        async fn is_raw(&self, string_literal: &ast::StringLiteral) -> Result<bool, Self::Error>;

        /// Compares the source contents without quotes with the decoded literal contents.
        #[operation(local)]
        async fn contents_match(
            &self,
            source: &SourceText,
            string_literal: &ast::StringLiteral,
        ) -> Result<bool, Self::Error>;

        /// Parses and indexes the annotation with `parsed_string_annotation`.
        ///
        /// The outer error reports an effect failure; the inner error reports rejected syntax
        /// or an indexing failure. Controlled effects run only this producer in structural preparation.
        #[operation(source)]
        async fn parse(
            &self,
            source: &SourceText,
            string_literal: &ast::StringLiteral,
        ) -> Result<Result<Parsed<ModExpression>, ParseError>, Self::Error>;

        /// Reports that the annotation uses a raw string.
        #[operation(child)]
        async fn raw_diagnostic(
            &self,
            context: &InferContext<'db, 'ast>,
            inference_flags: InferenceFlags,
            string_literal: &ast::StringLiteral,
        ) -> Result<(), Self::Error>;

        /// Reports that the annotation spans multiple string literals.
        #[operation(child)]
        async fn concatenation_diagnostic(
            &self,
            context: &InferContext<'db, 'ast>,
            string_expr: &ast::ExprStringLiteral,
        ) -> Result<(), Self::Error>;

        /// Reports that decoding escapes changes the annotation's source contents.
        #[operation(child)]
        async fn escape_diagnostic(
            &self,
            context: &InferContext<'db, 'ast>,
            inference_flags: InferenceFlags,
            string_expr: &ast::ExprStringLiteral,
        ) -> Result<(), Self::Error>;

        /// Reports a parser or indexing error, including applicable annotations and a `Literal` fix.
        #[operation(child)]
        async fn syntax_diagnostic(
            &self,
            context: &InferContext<'db, 'ast>,
            string_expr: &ast::ExprStringLiteral,
            string_literal: &ast::StringLiteral,
            error: &ParseError,
        ) -> Result<(), Self::Error>;
    }

    /// Validates and parses a quoted type expression, reporting rejected inputs through `effects`.
    ///
    /// A rejected input returns `None` only after its diagnostic effect succeeds. An interrupted or
    /// unavailable effect remains an error, including when diagnostic emission is unavailable.
    #[synchronous(parse_string_annotation_sync)]
    #[capabilities(effects = StringAnnotationEffects)]
    #[passive_values()]
    pub(in crate::types) async fn parse_string_annotation_with<'db, 'ast, E: StringAnnotationEffects<'db, 'ast>>(
        context: &InferContext<'db, 'ast>,
        inference_flags: InferenceFlags,
        string_expr: &ast::ExprStringLiteral,
        effects: &E,
    ) -> Result<Option<Parsed<ModExpression>>, E::Error> {
        let source = effects.source(context).await?;

        if let Some(string_literal) = effects.single_part(string_expr).await? {
            if effects.is_raw(string_literal).await? {
                effects.raw_diagnostic(context, inference_flags, string_literal).await?;
            } else if effects.contents_match(&source, string_literal).await? {
                match effects.parse(&source, string_literal).await? {
                    Ok(parsed) => return Ok(Some(parsed)),
                    Err(error) => {
                        effects.syntax_diagnostic(context, string_expr, string_literal, &error).await?;
                    }
                }
            } else {
                effects.escape_diagnostic(context, inference_flags, string_expr).await?;
            }
        } else {
            effects.concatenation_diagnostic(context, string_expr).await?;
        }

        Ok(None)
    }
}

/// Runs quoted-expression parsing and the existing lint diagnostics during ordinary inference.
#[derive(Debug)]
struct OrdinaryStringAnnotationEffects;

impl<'db, 'ast> SynchronousStringAnnotationEffects<'db, 'ast> for OrdinaryStringAnnotationEffects {
    type Error = Infallible;

    fn source(&self, context: &InferContext<'db, 'ast>) -> Result<SourceText, Infallible> {
        Ok(source_text(context.db(), context.file()))
    }

    fn single_part<'string>(
        &self,
        string_expr: &'string ast::ExprStringLiteral,
    ) -> Result<Option<&'string ast::StringLiteral>, Infallible> {
        Ok(string_expr.as_single_part_string())
    }

    fn is_raw(&self, string_literal: &ast::StringLiteral) -> Result<bool, Infallible> {
        Ok(string_literal.flags.prefix().is_raw())
    }

    fn contents_match(
        &self,
        source: &SourceText,
        string_literal: &ast::StringLiteral,
    ) -> Result<bool, Infallible> {
        // Compare the raw contents (without quotes) of the expression with the parsed contents
        // contained in the string literal.
        Ok(&source[string_literal.content_range()] == string_literal.as_str())
    }

    fn parse(
        &self,
        source: &SourceText,
        string_literal: &ast::StringLiteral,
    ) -> Result<Result<Parsed<ModExpression>, ParseError>, Infallible> {
        Ok(parsed_string_annotation(source.as_str(), string_literal))
    }

    fn raw_diagnostic(
        &self,
        context: &InferContext<'db, 'ast>,
        inference_flags: InferenceFlags,
        string_literal: &ast::StringLiteral,
    ) -> Result<(), Infallible> {
        if let Some(builder) = context.report_lint(&RAW_STRING_TYPE_ANNOTATION, string_literal) {
            builder.into_diagnostic(format_args!(
                "Raw string literals are not allowed in {}s",
                inference_flags.type_expression_context()
            ));
        }
        Ok(())
    }

    fn concatenation_diagnostic(
        &self,
        context: &InferContext<'db, 'ast>,
        string_expr: &ast::ExprStringLiteral,
    ) -> Result<(), Infallible> {
        if let Some(builder) =
            context.report_lint(&IMPLICIT_CONCATENATED_STRING_TYPE_ANNOTATION, string_expr)
        {
            // String is implicitly concatenated.
            builder.into_diagnostic("Type expressions cannot span multiple string literals");
        }
        Ok(())
    }

    fn escape_diagnostic(
        &self,
        context: &InferContext<'db, 'ast>,
        inference_flags: InferenceFlags,
        string_expr: &ast::ExprStringLiteral,
    ) -> Result<(), Infallible> {
        if let Some(builder) =
            context.report_lint(&ESCAPE_CHARACTER_IN_FORWARD_ANNOTATION, string_expr)
        {
            // The raw contents of the string doesn't match the parsed content. This could be the
            // case for annotations that contain escape sequences.
            builder.into_diagnostic(format_args!(
                "Escape characters are not allowed in {}s",
                inference_flags.type_expression_context()
            ));
        }
        Ok(())
    }

    fn syntax_diagnostic(
        &self,
        context: &InferContext<'db, 'ast>,
        string_expr: &ast::ExprStringLiteral,
        string_literal: &ast::StringLiteral,
        parse_error: &ParseError,
    ) -> Result<(), Infallible> {
        let ParseError { error, location } = parse_error;
        let location = *location;
        if let Some(builder) = context.report_lint(&INVALID_SYNTAX_IN_FORWARD_ANNOTATION, location)
        {
            let mut diagnostic = builder.into_diagnostic("Syntax error in forward annotation");

            diagnostic.set_primary_annotation_message(error);

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
                autofix_with_literal(context, &mut diagnostic, string_expr);
            }
        }
        Ok(())
    }
}
