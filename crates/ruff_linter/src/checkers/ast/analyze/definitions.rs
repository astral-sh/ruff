use ruff_python_semantic::all::DunderAllName;
use ruff_python_semantic::{
    BindingKind, ContextualizedDefinition, Definition, Export, Member, MemberKind,
};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::docstrings::Docstring;
use crate::fs::relativize_path;
use crate::rules::{flake8_annotations, flake8_pyi, pydoclint, pydocstyle, pylint};
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
        Rule::MissingTypeFunctionArgument,
        Rule::MissingTypeKwargs,
    ]);
    let enforce_stubs = checker.source_type.is_stub() && checker.enabled(Rule::DocstringInStub);
    let enforce_stubs_and_runtime = checker.enabled(Rule::IterMethodReturnIterable);
    let enforce_dunder_method = checker.enabled(Rule::BadDunderMethodName);
    let enforce_docstrings = checker.any_enabled(&[
        Rule::MissingBlankLineAfterLastSection,
        Rule::MissingBlankLineAfterSummary,
        Rule::BlankLineBeforeClass,
        Rule::BlankLinesBetweenHeaderAndContent,
        Rule::NonCapitalizedSectionName,
        Rule::MissingDashedUnderlineAfterSection,
        Rule::DocstringStartsWithThis,
        Rule::EmptyDocstring,
        Rule::EmptyDocstringSection,
        Rule::MissingTrailingPeriod,
        Rule::MissingTerminalPunctuation,
        Rule::EscapeSequenceInDocstring,
        Rule::FirstWordUncapitalized,
        Rule::UnnecessaryMultilineDocstring,
        Rule::DocstringTabIndentation,
        Rule::MultiLineSummaryFirstLine,
        Rule::MultiLineSummarySecondLine,
        Rule::NewLineAfterLastParagraph,
        Rule::MissingNewLineAfterSectionName,
        Rule::BlankLineAfterFunction,
        Rule::NoBlankLineAfterSection,
        Rule::BlankLineBeforeFunction,
        Rule::NoBlankLineBeforeSection,
        Rule::SignatureInDocstring,
        Rule::NonImperativeMood,
        Rule::IncorrectBlankLineAfterClass,
        Rule::IncorrectBlankLineBeforeClass,
        Rule::OverIndentation,
        Rule::OverloadWithDocstring,
        Rule::MissingSectionNameColon,
        Rule::OverindentedSection,
        Rule::MissingSectionUnderlineAfterName,
        Rule::MismatchedSectionUnderlineLength,
        Rule::OverindentedSectionUnderline,
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
    let enforce_pydoclint = checker.any_enabled(&[
        Rule::DocstringMissingReturns,
        Rule::DocstringExtraneousReturns,
        Rule::DocstringMissingYields,
        Rule::DocstringExtraneousYields,
        Rule::DocstringMissingException,
        Rule::DocstringExtraneousException,
    ]);

    if !enforce_annotations
        && !enforce_docstrings
        && !enforce_stubs
        && !enforce_stubs_and_runtime
        && !enforce_dunder_method
        && !enforce_pydoclint
    {
        return;
    }

    // Compute visibility of all definitions.
    let exports: Option<Vec<DunderAllName>> = {
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
    let mut overloaded_name: Option<&str> = None;
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
                    overloaded_name,
                    &checker.semantic,
                )
            }) {
                checker.report_diagnostics(flake8_annotations::rules::definition(
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
            flake8_pyi::rules::docstring_in_stubs(checker, definition, docstring);
        }
        if enforce_stubs_and_runtime {
            flake8_pyi::rules::iter_method_return_iterable(checker, definition);
        }

        // pylint
        if enforce_dunder_method {
            if let Definition::Member(Member {
                kind: MemberKind::Method(method),
                ..
            }) = definition
            {
                pylint::rules::bad_dunder_method_name(checker, method);
            }
        }

        // pydocstyle, pydoclint
        if enforce_docstrings || enforce_pydoclint {
            if pydocstyle::helpers::should_ignore_definition(
                definition,
                &checker.settings.pydocstyle,
                &checker.semantic,
            ) {
                continue;
            }

            // Extract a `Docstring` from a `Definition`.
            let Some(string_literal) = docstring else {
                pydocstyle::rules::not_missing(checker, definition, *visibility);
                continue;
            };

            // We don't recognise implicitly concatenated strings as valid docstrings in our model currently.
            let Some(sole_string_part) = string_literal.as_single_part_string() else {
                #[allow(deprecated)]
                let location = checker
                    .locator
                    .compute_source_location(string_literal.start());
                warn_user!(
                    "Docstring at {}:{}:{} contains implicit string concatenation; ignoring...",
                    relativize_path(checker.path),
                    location.row,
                    location.column
                );
                continue;
            };

            let docstring = Docstring {
                definition,
                expr: sole_string_part,
                source: checker.source(),
            };

            if !pydocstyle::rules::not_empty(checker, &docstring) {
                continue;
            }
            if checker.enabled(Rule::UnnecessaryMultilineDocstring) {
                pydocstyle::rules::one_liner(checker, &docstring);
            }
            if checker.any_enabled(&[Rule::BlankLineAfterFunction, Rule::BlankLineBeforeFunction]) {
                pydocstyle::rules::blank_before_after_function(checker, &docstring);
            }
            if checker.any_enabled(&[
                Rule::BlankLineBeforeClass,
                Rule::IncorrectBlankLineAfterClass,
                Rule::IncorrectBlankLineBeforeClass,
            ]) {
                pydocstyle::rules::blank_before_after_class(checker, &docstring);
            }
            if checker.enabled(Rule::MissingBlankLineAfterSummary) {
                pydocstyle::rules::blank_after_summary(checker, &docstring);
            }
            if checker.any_enabled(&[
                Rule::DocstringTabIndentation,
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
            if checker.enabled(Rule::MissingTrailingPeriod) {
                pydocstyle::rules::ends_with_period(checker, &docstring);
            }
            if checker.enabled(Rule::NonImperativeMood) {
                pydocstyle::rules::non_imperative_mood(
                    checker,
                    &docstring,
                    &checker.settings.pydocstyle,
                );
            }
            if checker.enabled(Rule::SignatureInDocstring) {
                pydocstyle::rules::no_signature(checker, &docstring);
            }
            if checker.enabled(Rule::FirstWordUncapitalized) {
                pydocstyle::rules::capitalized(checker, &docstring);
            }
            if checker.enabled(Rule::DocstringStartsWithThis) {
                pydocstyle::rules::starts_with_this(checker, &docstring);
            }
            if checker.enabled(Rule::MissingTerminalPunctuation) {
                pydocstyle::rules::ends_with_punctuation(checker, &docstring);
            }
            if checker.enabled(Rule::OverloadWithDocstring) {
                pydocstyle::rules::if_needed(checker, &docstring);
            }

            let enforce_sections = checker.any_enabled(&[
                Rule::MissingBlankLineAfterLastSection,
                Rule::BlankLinesBetweenHeaderAndContent,
                Rule::NonCapitalizedSectionName,
                Rule::MissingDashedUnderlineAfterSection,
                Rule::EmptyDocstringSection,
                Rule::MultiLineSummaryFirstLine,
                Rule::MissingNewLineAfterSectionName,
                Rule::NoBlankLineAfterSection,
                Rule::NoBlankLineBeforeSection,
                Rule::MissingSectionNameColon,
                Rule::OverindentedSection,
                Rule::MissingSectionUnderlineAfterName,
                Rule::MismatchedSectionUnderlineLength,
                Rule::OverindentedSectionUnderline,
                Rule::UndocumentedParam,
            ]);
            if enforce_sections || enforce_pydoclint {
                let section_contexts = pydocstyle::helpers::get_section_contexts(
                    &docstring,
                    checker.settings.pydocstyle.convention(),
                );

                if enforce_sections {
                    pydocstyle::rules::sections(
                        checker,
                        &docstring,
                        &section_contexts,
                        checker.settings.pydocstyle.convention(),
                    );
                }

                if enforce_pydoclint {
                    pydoclint::rules::check_docstring(
                        checker,
                        definition,
                        &docstring,
                        &section_contexts,
                        checker.settings.pydocstyle.convention(),
                    );
                }
            }
        }
    }
}
