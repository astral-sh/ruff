use ruff_python_ast::str::raw_contents_range;
use ruff_text_size::{Ranged, TextRange};

use ruff_python_semantic::{BindingKind, ContextualizedDefinition, Export};

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::docstrings::Docstring;
use crate::fs::relativize_path;
use crate::rules::{flake8_annotations, flake8_pyi, pydocstyle};
use crate::{docstrings, warn_user};

/// Run lint rules over all [`Definition`] nodes in the [`SemanticModel`].
///
/// This phase is expected to run after the AST has been traversed in its entirety; as such,
/// it is expected that all [`Definition`] nodes have been visited by the time, and that this
/// method will not recurse into any other nodes.
pub(crate) fn definitions(checker: &mut Checker) {
    let enforce_annotations = checker.any_enabled(&[
        Rule::AnyType,
        Rule::MissingReturnTypeClassMethod,
        Rule::MissingReturnTypePrivateFunction,
        Rule::MissingReturnTypeSpecialMethod,
        Rule::MissingReturnTypeStaticMethod,
        Rule::MissingReturnTypeUndocumentedPublicFunction,
        Rule::MissingTypeArgs,
        Rule::MissingTypeCls,
        Rule::MissingTypeFunctionArgument,
        Rule::MissingTypeKwargs,
        Rule::MissingTypeSelf,
    ]);
    let enforce_stubs = checker.source_type.is_stub() && checker.enabled(Rule::DocstringInStub);
    let enforce_stubs_and_runtime = checker.enabled(Rule::IterMethodReturnIterable);
    let enforce_docstrings = checker.any_enabled(&[
        Rule::BlankLineAfterLastSection,
        Rule::BlankLineAfterSummary,
        Rule::BlankLineBeforeClass,
        Rule::BlankLinesBetweenHeaderAndContent,
        Rule::CapitalizeSectionName,
        Rule::DashedUnderlineAfterSection,
        Rule::DocstringStartsWithThis,
        Rule::EmptyDocstring,
        Rule::EmptyDocstringSection,
        Rule::EndsInPeriod,
        Rule::EndsInPunctuation,
        Rule::EscapeSequenceInDocstring,
        Rule::FirstLineCapitalized,
        Rule::FitsOnOneLine,
        Rule::IndentWithSpaces,
        Rule::MultiLineSummaryFirstLine,
        Rule::MultiLineSummarySecondLine,
        Rule::NewLineAfterLastParagraph,
        Rule::NewLineAfterSectionName,
        Rule::NoBlankLineAfterFunction,
        Rule::NoBlankLineAfterSection,
        Rule::NoBlankLineBeforeFunction,
        Rule::NoBlankLineBeforeSection,
        Rule::NoSignature,
        Rule::NonImperativeMood,
        Rule::OneBlankLineAfterClass,
        Rule::OneBlankLineBeforeClass,
        Rule::OverIndentation,
        Rule::OverloadWithDocstring,
        Rule::SectionNameEndsInColon,
        Rule::SectionNotOverIndented,
        Rule::SectionUnderlineAfterName,
        Rule::SectionUnderlineMatchesSectionLength,
        Rule::SectionUnderlineNotOverIndented,
        Rule::SurroundingWhitespace,
        Rule::TripleSingleQuotes,
        Rule::UnderIndentation,
        Rule::UndocumentedMagicMethod,
        Rule::UndocumentedParam,
        Rule::UndocumentedPublicClass,
        Rule::UndocumentedPublicFunction,
        Rule::UndocumentedPublicInit,
        Rule::UndocumentedPublicMethod,
        Rule::UndocumentedPublicModule,
        Rule::UndocumentedPublicNestedClass,
        Rule::UndocumentedPublicPackage,
    ]);

    if !enforce_annotations && !enforce_docstrings && !enforce_stubs && !enforce_stubs_and_runtime {
        return;
    }

    // Compute visibility of all definitions.
    let exports: Option<Vec<&str>> = {
        checker
            .semantic
            .global_scope()
            .get_all("__all__")
            .map(|binding_id| &checker.semantic.bindings[binding_id])
            .filter_map(|binding| match &binding.kind {
                BindingKind::Export(Export { names }) => Some(names.iter().copied()),
                _ => None,
            })
            .fold(None, |acc, names| {
                Some(acc.into_iter().flatten().chain(names).collect())
            })
    };

    let definitions = std::mem::take(&mut checker.semantic.definitions);
    let mut overloaded_name: Option<String> = None;
    for ContextualizedDefinition {
        definition,
        visibility,
    } in definitions.resolve(exports.as_deref()).iter()
    {
        let docstring = docstrings::extraction::extract_docstring(definition);

        // flake8-annotations
        if enforce_annotations {
            // TODO(charlie): This should be even stricter, in that an overload
            // implementation should come immediately after the overloaded
            // interfaces, without any AST nodes in between. Right now, we
            // only error when traversing definition boundaries (functions,
            // classes, etc.).
            if !overloaded_name.is_some_and(|overloaded_name| {
                flake8_annotations::helpers::is_overload_impl(
                    definition,
                    &overloaded_name,
                    &checker.semantic,
                )
            }) {
                checker
                    .diagnostics
                    .extend(flake8_annotations::rules::definition(
                        checker,
                        definition,
                        *visibility,
                    ));
            }
            overloaded_name =
                flake8_annotations::helpers::overloaded_name(definition, &checker.semantic);
        }

        // flake8-pyi
        if enforce_stubs {
            if checker.enabled(Rule::DocstringInStub) {
                flake8_pyi::rules::docstring_in_stubs(checker, docstring);
            }
        }
        if enforce_stubs_and_runtime {
            if checker.enabled(Rule::IterMethodReturnIterable) {
                flake8_pyi::rules::iter_method_return_iterable(checker, definition);
            }
        }

        // pydocstyle
        if enforce_docstrings {
            if pydocstyle::helpers::should_ignore_definition(
                definition,
                &checker.settings.pydocstyle.ignore_decorators,
                &checker.semantic,
            ) {
                continue;
            }

            // Extract a `Docstring` from a `Definition`.
            let Some(expr) = docstring else {
                pydocstyle::rules::not_missing(checker, definition, *visibility);
                continue;
            };

            let contents = checker.locator().slice(expr);

            let indentation = checker.locator().slice(TextRange::new(
                checker.locator.line_start(expr.start()),
                expr.start(),
            ));

            if pydocstyle::helpers::should_ignore_docstring(expr) {
                #[allow(deprecated)]
                let location = checker.locator.compute_source_location(expr.start());
                warn_user!(
                    "Docstring at {}:{}:{} contains implicit string concatenation; ignoring...",
                    relativize_path(checker.path),
                    location.row,
                    location.column
                );
                continue;
            }

            // SAFETY: Safe for docstrings that pass `should_ignore_docstring`.
            let body_range = raw_contents_range(contents).unwrap();
            let docstring = Docstring {
                definition,
                expr,
                contents,
                body_range,
                indentation,
            };

            if !pydocstyle::rules::not_empty(checker, &docstring) {
                continue;
            }
            if checker.enabled(Rule::FitsOnOneLine) {
                pydocstyle::rules::one_liner(checker, &docstring);
            }
            if checker.any_enabled(&[
                Rule::NoBlankLineAfterFunction,
                Rule::NoBlankLineBeforeFunction,
            ]) {
                pydocstyle::rules::blank_before_after_function(checker, &docstring);
            }
            if checker.any_enabled(&[
                Rule::BlankLineBeforeClass,
                Rule::OneBlankLineAfterClass,
                Rule::OneBlankLineBeforeClass,
            ]) {
                pydocstyle::rules::blank_before_after_class(checker, &docstring);
            }
            if checker.enabled(Rule::BlankLineAfterSummary) {
                pydocstyle::rules::blank_after_summary(checker, &docstring);
            }
            if checker.any_enabled(&[
                Rule::IndentWithSpaces,
                Rule::OverIndentation,
                Rule::UnderIndentation,
            ]) {
                pydocstyle::rules::indent(checker, &docstring);
            }
            if checker.enabled(Rule::NewLineAfterLastParagraph) {
                pydocstyle::rules::newline_after_last_paragraph(checker, &docstring);
            }
            if checker.enabled(Rule::SurroundingWhitespace) {
                pydocstyle::rules::no_surrounding_whitespace(checker, &docstring);
            }
            if checker.any_enabled(&[
                Rule::MultiLineSummaryFirstLine,
                Rule::MultiLineSummarySecondLine,
            ]) {
                pydocstyle::rules::multi_line_summary_start(checker, &docstring);
            }
            if checker.enabled(Rule::TripleSingleQuotes) {
                pydocstyle::rules::triple_quotes(checker, &docstring);
            }
            if checker.enabled(Rule::EscapeSequenceInDocstring) {
                pydocstyle::rules::backslashes(checker, &docstring);
            }
            if checker.enabled(Rule::EndsInPeriod) {
                pydocstyle::rules::ends_with_period(checker, &docstring);
            }
            if checker.enabled(Rule::NonImperativeMood) {
                pydocstyle::rules::non_imperative_mood(
                    checker,
                    &docstring,
                    &checker.settings.pydocstyle.property_decorators,
                );
            }
            if checker.enabled(Rule::NoSignature) {
                pydocstyle::rules::no_signature(checker, &docstring);
            }
            if checker.enabled(Rule::FirstLineCapitalized) {
                pydocstyle::rules::capitalized(checker, &docstring);
            }
            if checker.enabled(Rule::DocstringStartsWithThis) {
                pydocstyle::rules::starts_with_this(checker, &docstring);
            }
            if checker.enabled(Rule::EndsInPunctuation) {
                pydocstyle::rules::ends_with_punctuation(checker, &docstring);
            }
            if checker.enabled(Rule::OverloadWithDocstring) {
                pydocstyle::rules::if_needed(checker, &docstring);
            }
            if checker.any_enabled(&[
                Rule::BlankLineAfterLastSection,
                Rule::BlankLinesBetweenHeaderAndContent,
                Rule::CapitalizeSectionName,
                Rule::DashedUnderlineAfterSection,
                Rule::EmptyDocstringSection,
                Rule::MultiLineSummaryFirstLine,
                Rule::NewLineAfterSectionName,
                Rule::NoBlankLineAfterSection,
                Rule::NoBlankLineBeforeSection,
                Rule::SectionNameEndsInColon,
                Rule::SectionNotOverIndented,
                Rule::SectionUnderlineAfterName,
                Rule::SectionUnderlineMatchesSectionLength,
                Rule::SectionUnderlineNotOverIndented,
                Rule::UndocumentedParam,
            ]) {
                pydocstyle::rules::sections(
                    checker,
                    &docstring,
                    checker.settings.pydocstyle.convention.as_ref(),
                );
            }
        }
    }
}
