use std::collections::HashMap;
use std::sync::LazyLock;

use crate::registry::Rule;

/// Returns the redirect target for a rule code, prefix, or name.
pub(crate) fn get_redirect_target(selector: &str) -> Option<&'static str> {
    REDIRECTS.get(selector).copied()
}

/// Returns the original selector and its redirect target, both with a static lifetime.
pub(crate) fn get_redirect(selector: &str) -> Option<(&'static str, &'static str)> {
    REDIRECTS.get_key_value(selector).map(|(k, v)| (*k, *v))
}

/// Former rule names and their current rules.
pub const RULE_NAME_REDIRECTS: &[(&str, Rule)] = &[
    (
        "missing-return-type-undocumented-public-function",
        Rule::MissingReturnAnnotationForPublicFunction,
    ),
    (
        "blank-line-before-function",
        Rule::BlankLineBeforeFunctionDocstring,
    ),
    (
        "blank-line-after-function",
        Rule::BlankLineAfterFunctionDocstring,
    ),
    (
        "incorrect-blank-line-before-class",
        Rule::IncorrectBlankLineBeforeClassDocstring,
    ),
    (
        "incorrect-blank-line-after-class",
        Rule::IncorrectBlankLineAfterClassDocstring,
    ),
    (
        "missing-blank-line-after-summary",
        Rule::IncorrectBlankLineAfterDocstringSummary,
    ),
    ("under-indentation", Rule::UnderIndentedDocstringLine),
    ("over-indentation", Rule::OverIndentedDocstringLine),
    (
        "new-line-after-last-paragraph",
        Rule::MissingNewlineAfterLastDocstringParagraph,
    ),
    (
        "missing-new-line-after-last-docstring-paragraph",
        Rule::MissingNewlineAfterLastDocstringParagraph,
    ),
    (
        "surrounding-whitespace",
        Rule::WhitespaceAroundDocstringText,
    ),
    (
        "blank-line-before-class",
        Rule::BlankLineBeforeClassDocstring,
    ),
    (
        "multi-line-summary-first-line",
        Rule::MultilineDocstringSummaryOnSeparateLine,
    ),
    (
        "multi-line-summary-second-line",
        Rule::MultilineDocstringSummaryOnFirstLine,
    ),
    ("overindented-section", Rule::OverIndentedDocstringSection),
    (
        "overindented-section-underline",
        Rule::OverIndentedDocstringSectionUnderline,
    ),
    ("triple-single-quotes", Rule::UnconventionalDocstringQuotes),
    (
        "missing-trailing-period",
        Rule::DocstringSummaryWithoutPeriod,
    ),
    ("non-imperative-mood", Rule::NonImperativeDocstringSummary),
    (
        "first-word-uncapitalized",
        Rule::DocstringStartsWithLowercaseLetter,
    ),
    (
        "non-capitalized-section-name",
        Rule::IncorrectlyCapitalizedDocstringSectionName,
    ),
    (
        "missing-new-line-after-section-name",
        Rule::MissingNewlineAfterDocstringSectionName,
    ),
    (
        "missing-dashed-underline-after-section",
        Rule::MissingDocstringSectionUnderline,
    ),
    (
        "missing-section-underline-after-name",
        Rule::BlankLineBeforeDocstringSectionUnderline,
    ),
    (
        "mismatched-section-underline-length",
        Rule::IncorrectDocstringSectionUnderlineLength,
    ),
    (
        "no-blank-line-after-section",
        Rule::MissingBlankLineAfterDocstringSection,
    ),
    (
        "no-blank-line-before-section",
        Rule::MissingBlankLineBeforeDocstringSection,
    ),
    (
        "blank-lines-between-header-and-content",
        Rule::ExtraBlankLineBeforeDocstringSectionContent,
    ),
    (
        "missing-blank-line-after-last-section",
        Rule::MissingBlankLineAfterLastDocstringSection,
    ),
    (
        "missing-terminal-punctuation",
        Rule::DocstringSummaryWithoutTerminalPunctuation,
    ),
    (
        "missing-section-name-colon",
        Rule::DocstringSectionNameWithoutColon,
    ),
    ("undocumented-param", Rule::UndocumentedParameter),
    (
        "incorrect-section-order",
        Rule::IncorrectDocstringSectionOrder,
    ),
    (
        "docstring-extraneous-parameter",
        Rule::ExtraneousDocstringParameter,
    ),
    (
        "docstring-missing-returns",
        Rule::MissingDocstringReturnsSection,
    ),
    (
        "docstring-extraneous-returns",
        Rule::ExtraneousDocstringReturnsSection,
    ),
    (
        "docstring-missing-yields",
        Rule::MissingDocstringYieldsSection,
    ),
    (
        "docstring-extraneous-yields",
        Rule::ExtraneousDocstringYieldsSection,
    ),
    (
        "docstring-missing-exception",
        Rule::UndocumentedRaisedException,
    ),
    (
        "docstring-extraneous-exception",
        Rule::ExtraneousDocstringException,
    ),
    ("get-attr-with-constant", Rule::GetattrWithConstant),
    ("set-attr-with-constant", Rule::SetattrWithConstant),
    ("del-attr-with-constant", Rule::DelattrWithConstant),
    ("f-string-in-get-text-func-call", Rule::FStringInGettextCall),
    ("format-in-get-text-func-call", Rule::StrFormatInGettextCall),
    ("printf-in-get-text-func-call", Rule::PrintfInGettextCall),
    ("exec-builtin", Rule::Exec),
    ("suspicious-eval-usage", Rule::Eval),
    ("suspicious-telnet-usage", Rule::SuspiciousTelnetlibUsage),
    ("suspicious-ftp-lib-usage", Rule::SuspiciousFtplibUsage),
    ("p-print", Rule::Pprint),
    ("raw-string-in-exception", Rule::StringLiteralInException),
    ("dot-format-in-exception", Rule::StrFormatInException),
    ("logging-exc-info", Rule::LoggingErrorWithExcInfo),
    ("invalid-get-logger-argument", Rule::FilePathAsLoggerName),
    (
        "log-exception-outside-except-handler",
        Rule::LoggingExceptionOutsideExceptHandler,
    ),
    (
        "exception-without-exc-info",
        Rule::LoggingExceptionWithFalsyExcInfo,
    ),
    (
        "exc-info-outside-except-handler",
        Rule::LoggingExcInfoOutsideExceptHandler,
    ),
    ("undocumented-warn", Rule::LoggingWarnConstant),
    ("raise-vanilla-class", Rule::RaiseExceptionOrBaseException),
    ("raise-vanilla-args", Rule::RaiseExceptionWithMessage),
    (
        "error-instead-of-exception",
        Rule::LoggingErrorInsteadOfException,
    ),
    (
        "verbose-log-message",
        Rule::RedundantExceptionInLoggingMessage,
    ),
    (
        "boolean-type-hint-positional-argument",
        Rule::BooleanAnnotatedPositionalParameter,
    ),
    (
        "boolean-default-value-positional-argument",
        Rule::BooleanPositionalParameterDefault,
    ),
    ("invalid-class-name", Rule::NonCamelCaseClassName),
    ("invalid-function-name", Rule::NonLowercaseFunctionName),
    ("invalid-argument-name", Rule::NonLowercaseParameterName),
    (
        "invalid-first-argument-name-for-class-method",
        Rule::ClassMethodReceiverNotNamedCls,
    ),
    (
        "invalid-first-argument-name-for-method",
        Rule::InstanceMethodReceiverNotNamedSelf,
    ),
    (
        "camelcase-imported-as-lowercase",
        Rule::CamelCaseImportedAsLowercase,
    ),
    (
        "camelcase-imported-as-constant",
        Rule::CamelCaseImportedAsConstant,
    ),
    (
        "camelcase-imported-as-acronym",
        Rule::CamelCaseImportedAsAcronym,
    ),
    (
        "error-suffix-on-exception-name",
        Rule::ExceptionClassWithoutErrorSuffix,
    ),
    ("no-classmethod-decorator", Rule::NonDecoratorClassmethod),
    ("no-staticmethod-decorator", Rule::NonDecoratorStaticmethod),
    ("no-self-use", Rule::UnusedSelf),
    (
        "typed-argument-default-in-stub",
        Rule::ComplexDefaultForAnnotatedParameterInStub,
    ),
    (
        "argument-default-in-stub",
        Rule::ComplexDefaultForUnannotatedParameterInStub,
    ),
    (
        "type-alias-without-annotation",
        Rule::UnannotatedTypeAliasInStub,
    ),
    (
        "no-return-argument-annotation-in-stub",
        Rule::ParameterAnnotatedWithNoReturn,
    ),
    ("redundant-literal-union", Rule::LiteralUnionWithSupertype),
    ("byte-string-usage", Rule::DeprecatedByteStringType),
    (
        "generator-return-from-iter-method",
        Rule::IterMethodWithGeneratorReturnAnnotation,
    ),
    ("runtime-cast-value", Rule::CastWithUnquotedType),
    (
        "convert-typed-dict-functional-to-class",
        Rule::FunctionalTypedDict,
    ),
    (
        "convert-named-tuple-functional-to-class",
        Rule::FunctionalNamedTuple,
    ),
    ("replace-str-enum", Rule::ManualStrEnum),
    ("prohibited-trailing-comma", Rule::UnnecessaryTrailingComma),
    (
        "blank-line-between-methods",
        Rule::MissingBlankLineBeforeMethod,
    ),
    (
        "blank-lines-top-level",
        Rule::MissingBlankLinesBeforeTopLevelDefinition,
    ),
    (
        "blank-lines-after-function-or-class",
        Rule::MissingBlankLinesAfterTopLevelDefinition,
    ),
    (
        "blank-lines-before-nested-definition",
        Rule::MissingBlankLineBeforeNestedDefinition,
    ),
    ("shebang-not-executable", Rule::NonExecutableFileWithShebang),
    (
        "shebang-missing-executable-file",
        Rule::ExecutableFileWithoutShebang,
    ),
    ("undefined-local-with-import-star", Rule::StarImport),
    (
        "undefined-local-with-import-star-usage",
        Rule::PossiblyUndefinedNameWithStarImport,
    ),
    (
        "undefined-local-with-nested-import-star-usage",
        Rule::NonModuleLevelStarImport,
    ),
    (
        "invalid-character-backspace",
        Rule::UnescapedBackspaceCharacter,
    ),
    ("invalid-character-sub", Rule::UnescapedSubstituteCharacter),
    ("invalid-character-esc", Rule::UnescapedEscapeCharacter),
    ("invalid-character-nul", Rule::UnescapedNullCharacter),
    (
        "invalid-character-zero-width-space",
        Rule::UnescapedZeroWidthSpaceCharacter,
    ),
    ("unused-noqa", Rule::UnusedSuppressionComment),
    ("redirected-noqa", Rule::RenamedRuleInSuppressionComment),
    ("invalid-rule-code", Rule::UnknownRuleInSuppressionComment),
    (
        "star-arg-unpacking-after-keyword-arg",
        Rule::StarArgAfterKeyword,
    ),
    (
        "if-exp-instead-of-or-operator",
        Rule::IfExprWithBodyAsCondition,
    ),
    (
        "repeated-global",
        Rule::ConsecutiveGlobalOrNonlocalStatements,
    ),
    ("stop-iteration-return", Rule::RaiseStopIterationInGenerator),
    ("assert-on-string-literal", Rule::AssertStringLiteral),
    ("redefined-loop-name", Rule::RedefinedForOrWithTarget),
    ("superfluous-else-return", Rule::UnnecessaryElseAfterReturn),
    ("superfluous-else-raise", Rule::UnnecessaryElseAfterRaise),
    (
        "superfluous-else-continue",
        Rule::UnnecessaryElseAfterContinue,
    ),
    ("superfluous-else-break", Rule::UnnecessaryElseAfterBreak),
    (
        "parenthesize-chained-operators",
        Rule::UnclearBooleanPrecedence,
    ),
    (
        "missing-f-string-syntax",
        Rule::PossiblyMissingFStringPrefix,
    ),
    ("useless-if-else", Rule::IfExprWithIdenticalBranches),
    (
        "if-else-block-instead-of-if-exp",
        Rule::IfElseAssignmentToSameVariable,
    ),
    ("enumerate-for-loop", Rule::ManualLoopCounter),
    ("if-with-same-arms", Rule::IfWithIdenticalBranches),
    (
        "if-expr-with-twisted-arms",
        Rule::IfExprWithNegatedElseAsCondition,
    ),
    ("trio-sync-call", Rule::TrioCallNotAwaited),
    (
        "blocking-path-method-in-async-function",
        Rule::BlockingPathCallInAsyncFunction,
    ),
    (
        "strip-with-multi-characters",
        Rule::StripWithDuplicateCharacters,
    ),
    ("static-join-to-f-string", Rule::UnnecessaryStrJoin),
    ("read-whole-file", Rule::ManualPathRead),
    ("write-whole-file", Rule::ManualPathWrite),
    ("f-string-number-format", Rule::SlicedIntegerBaseConversion),
    ("math-constant", Rule::HardcodedMathConstant),
    ("invalid-envvar-value", Rule::InvalidGetenvKey),
    ("legacy-form-pytest-raises", Rule::PytestLegacyCallableForm),
    (
        "hardcoded-sql-expression",
        Rule::SuspiciousSqlStringConstruction,
    ),
    (
        "open-file-with-context-handler",
        Rule::OpenFileWithoutContextManager,
    ),
    ("native-literals", Rule::UnnecessaryLiteralConstructor),
    (
        "replace-universal-newlines",
        Rule::SubprocessRunWithUniversalNewlines,
    ),
    (
        "replace-stdout-stderr",
        Rule::SubprocessRunWithPipedStdoutAndStderr,
    ),
    (
        "format-literals",
        Rule::UnnecessaryPositionalStrFormatIndices,
    ),
    ("f-string", Rule::StrFormat),
    (
        "string-dot-format-invalid-format",
        Rule::StrFormatInvalidSyntax,
    ),
    (
        "string-dot-format-extra-named-arguments",
        Rule::StrFormatUnusedKeywordArguments,
    ),
    (
        "string-dot-format-extra-positional-arguments",
        Rule::StrFormatUnusedPositionalArguments,
    ),
    (
        "string-dot-format-missing-arguments",
        Rule::StrFormatMissingArguments,
    ),
    (
        "string-dot-format-mixing-automatic",
        Rule::StrFormatMixedPlaceholderNumbering,
    ),
    ("blind-except", Rule::CatchExceptionOrBaseException),
    (
        "assert-raises-exception",
        Rule::AssertRaisesExceptionOrBaseException,
    ),
    ("logging-string-format", Rule::LoggingStrFormat),
    (
        "builtin-argument-shadowing",
        Rule::BuiltinParameterShadowing,
    ),
    (
        "builtin-lambda-argument-shadowing",
        Rule::BuiltinLambdaParameterShadowing,
    ),
    ("missing-type-function-argument", Rule::UnannotatedParameter),
    ("missing-type-args", Rule::UnannotatedArgs),
    ("missing-type-kwargs", Rule::UnannotatedKwargs),
    ("unused-function-argument", Rule::UnusedFunctionParameter),
    ("unused-method-argument", Rule::UnusedMethodParameter),
    (
        "unused-class-method-argument",
        Rule::UnusedClassMethodParameter,
    ),
    (
        "unused-static-method-argument",
        Rule::UnusedStaticMethodParameter,
    ),
    ("unused-lambda-argument", Rule::UnusedLambdaParameter),
    ("mutable-argument-default", Rule::MutableParameterDefault),
    (
        "function-call-in-default-argument",
        Rule::FunctionCallInParameterDefault,
    ),
    ("no-method-argument", Rule::MethodWithoutParameters),
    ("bad-staticmethod-argument", Rule::BadStaticmethodParameter),
    ("too-many-arguments", Rule::TooManyParameters),
    (
        "too-many-positional-arguments",
        Rule::TooManyPositionalParameters,
    ),
    (
        "redefined-argument-from-local",
        Rule::RedefinedParameterFromLocal,
    ),
    (
        "pytest-use-fixtures-without-parameters",
        Rule::PytestUseFixturesWithoutArguments,
    ),
    (
        "pytest-parameter-with-default-argument",
        Rule::PytestParameterWithDefaultValue,
    ),
    ("super-call-with-parameters", Rule::RedundantSuperArguments),
    (
        "lru-cache-without-parameters",
        Rule::LRUCacheCallWithoutArguments,
    ),
    (
        "whitespace-before-parameters",
        Rule::WhitespaceBeforeOpenBracket,
    ),
    (
        "unexpected-spaces-around-keyword-parameter-equals",
        Rule::UnexpectedSpacesAroundArgumentOrParameterEquals,
    ),
    (
        "function-call-in-dataclass-default-argument",
        Rule::FunctionCallInDataclassFieldDefault,
    ),
    (
        "invalid-assert-message-literal-argument",
        Rule::NonStringLiteralAsAssertMessage,
    ),
    ("unprefixed-type-param", Rule::PublicTypeVarInStub),
    (
        "unrecognized-version-info-check",
        Rule::UnrecognizedVersionInfoCheckInStub,
    ),
    (
        "patch-version-comparison",
        Rule::PatchVersionComparisonInStub,
    ),
    (
        "wrong-tuple-length-version-comparison",
        Rule::WrongTupleLengthVersionComparisonInStub,
    ),
    (
        "unrecognized-platform-check",
        Rule::UnrecognizedPlatformCheckInStub,
    ),
    (
        "unrecognized-platform-name",
        Rule::UnrecognizedPlatformNameInStub,
    ),
    ("pass-in-class-body", Rule::PassInStubClassBody),
    ("string-or-bytes-too-long", Rule::StringOrBytesTooLongInStub),
    (
        "numeric-literal-too-long",
        Rule::NumericLiteralTooLongInStub,
    ),
    (
        "missing-return-type-public-function",
        Rule::MissingReturnAnnotationForPublicFunction,
    ),
    (
        "missing-return-type-private-function",
        Rule::MissingReturnAnnotationForPrivateFunction,
    ),
    (
        "missing-return-type-special-method",
        Rule::MissingReturnAnnotationForSpecialMethod,
    ),
    (
        "missing-return-type-static-method",
        Rule::MissingReturnAnnotationForStaticMethod,
    ),
    (
        "missing-return-type-class-method",
        Rule::MissingReturnAnnotationForClassMethod,
    ),
    (
        "percent-format-invalid-format",
        Rule::PercentFormatInvalidSyntax,
    ),
    (
        "percent-format-extra-named-arguments",
        Rule::PercentFormatUnusedNamedArguments,
    ),
    ("logging-too-many-args", Rule::LoggingTooManyArguments),
    ("logging-too-few-args", Rule::LoggingTooFewArguments),
    ("re-sub-positional-args", Rule::ReSubPositionalArguments),
    ("format-in-gettext-call", Rule::StrFormatInGettextCall),
    (
        "unnecessary-default-type-args",
        Rule::UnnecessaryDefaultTypeArguments,
    ),
    (
        "hardcoded-password-func-arg",
        Rule::HardcodedPasswordArgument,
    ),
    (
        "hardcoded-password-default",
        Rule::HardcodedPasswordParameterDefault,
    ),
    (
        "boolean-default-value-positional-parameter",
        Rule::BooleanPositionalParameterDefault,
    ),
    (
        "pytest-fixture-positional-args",
        Rule::PytestFixturePositionalArguments,
    ),
    (
        "pytest-fixture-param-without-value",
        Rule::PytestFixtureParameterWithoutValue,
    ),
    (
        "mutable-dataclass-default",
        Rule::MutableDataclassFieldDefault,
    ),
];

static REDIRECTS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut redirects = HashMap::from([
        // The following are here because we don't yet have the many-to-one mapping enabled.
        ("SIM111", "SIM110"),
        // The following are deprecated.
        ("C9", "C90"),
        ("T1", "T10"),
        ("T2", "T20"),
        // TODO(charlie): Remove by 2023-02-01.
        ("R", "RET"),
        ("R5", "RET5"),
        ("R50", "RET50"),
        ("R501", "RET501"),
        ("R502", "RET502"),
        ("R503", "RET503"),
        ("R504", "RET504"),
        ("R505", "RET505"),
        ("R506", "RET506"),
        ("R507", "RET507"),
        ("R508", "RET508"),
        ("IC", "ICN"),
        ("IC0", "ICN0"),
        ("IC00", "ICN00"),
        ("IC001", "ICN001"),
        ("IC002", "ICN001"),
        ("IC003", "ICN001"),
        ("IC004", "ICN001"),
        // TODO(charlie): Remove by 2023-01-01.
        ("U", "UP"),
        ("U0", "UP0"),
        ("U00", "UP00"),
        ("U001", "UP001"),
        ("U003", "UP003"),
        ("U004", "UP004"),
        ("U005", "UP005"),
        ("U006", "UP006"),
        ("U007", "UP007"),
        ("U008", "UP008"),
        ("U009", "UP009"),
        ("U01", "UP01"),
        ("U010", "UP010"),
        ("U011", "UP011"),
        ("U012", "UP012"),
        ("U013", "UP013"),
        ("U014", "UP014"),
        ("U015", "UP015"),
        ("U016", "UP016"),
        ("U017", "UP017"),
        ("U019", "UP019"),
        // TODO(charlie): Remove by 2023-02-01.
        ("I2", "TID2"),
        ("I25", "TID25"),
        ("I252", "TID252"),
        ("M", "RUF100"),
        ("M0", "RUF100"),
        ("M001", "RUF100"),
        // TODO(charlie): Remove by 2023-02-01.
        ("PDV", "PD"),
        ("PDV0", "PD0"),
        ("PDV002", "PD002"),
        ("PDV003", "PD003"),
        ("PDV004", "PD004"),
        ("PDV007", "PD007"),
        ("PDV008", "PD008"),
        ("PDV009", "PD009"),
        ("PDV01", "PD01"),
        ("PDV010", "PD010"),
        ("PDV011", "PD011"),
        ("PDV012", "PD012"),
        ("PDV013", "PD013"),
        ("PDV015", "PD015"),
        ("PDV9", "PD9"),
        ("PDV90", "PD90"),
        ("PDV901", "PD901"),
        // TODO(charlie): Remove by 2023-04-01.
        ("TYP", "TC"),
        ("TYP001", "TC001"),
        // TODO(charlie): Remove by 2023-06-01.
        ("RUF004", "B026"),
        ("PIE802", "C419"),
        ("PLW0130", "B033"),
        ("T001", "FIX001"),
        ("T002", "FIX002"),
        ("T003", "FIX003"),
        ("T004", "FIX004"),
        ("RUF011", "B035"),
        ("TRY200", "B904"),
        ("PGH001", "S307"),
        ("PGH002", "G010"),
        // flake8-trio and flake8-async merged with name flake8-async
        ("TRIO", "ASYNC1"),
        ("TRIO1", "ASYNC1"),
        ("TRIO10", "ASYNC10"),
        ("TRIO100", "ASYNC100"),
        ("TRIO105", "ASYNC105"),
        ("TRIO109", "ASYNC109"),
        ("TRIO11", "ASYNC11"),
        ("TRIO110", "ASYNC110"),
        ("TRIO115", "ASYNC115"),
        // Removed in v0.5
        ("PLR1701", "SIM101"),
        // Test redirect by exact code
        #[cfg(any(feature = "test-rules", test))]
        ("RUF940", "RUF950"),
        // Test redirect by prefix
        #[cfg(any(feature = "test-rules", test))]
        ("RUF96", "RUF95"),
        // See: https://github.com/astral-sh/ruff/issues/10791
        ("PLW0117", "PLW0177"),
        // See: https://github.com/astral-sh/ruff/issues/12110
        ("RUF025", "C420"),
        // See: https://github.com/astral-sh/ruff/issues/13492
        ("TRY302", "TRY203"),
        // TCH renamed to TC to harmonize with flake8 plugin
        ("TCH", "TC"),
        ("TCH001", "TC001"),
        ("TCH002", "TC002"),
        ("TCH003", "TC003"),
        ("TCH004", "TC004"),
        ("TCH005", "TC005"),
        ("TCH006", "TC010"),
        ("TCH010", "TC010"),
        ("RUF035", "S704"),
    ]);
    redirects.extend(
        RULE_NAME_REDIRECTS
            .iter()
            .map(|&(old_name, rule)| (old_name, rule.name().as_str())),
    );
    redirects
});

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::codes::{Rule, RuleStatus};
    use crate::rule_redirects::{REDIRECTS, RULE_NAME_REDIRECTS};
    use strum::IntoEnumIterator;

    /// Tests for rule names and codes that overlap with a redirect.
    #[test]
    fn overshadowing_redirects() {
        for rule in Rule::iter() {
            if matches!(rule.status(), RuleStatus::Removed { .. }) {
                continue;
            }

            let name = rule.name();
            if let Some(redirect_target) = REDIRECTS.get(name.as_str()) {
                panic!(
                    "Rule {name} is overshadowed by a redirect, which points to {redirect_target}."
                );
            }
            if let Some(code) = rule.noqa_code()
                && let Some(redirect_target) = REDIRECTS.get(&*code.to_string())
            {
                panic!(
                    "Rule {code} is overshadowed by a redirect, which points to {redirect_target}."
                );
            }
        }
    }

    /// Tests for redirected rule names and codes that overlap with each other.
    #[test]
    fn unique_redirects() {
        let codes = Rule::iter()
            .filter_map(|rule| rule.noqa_code())
            .flat_map(|code| {
                let code = code.to_string();
                [code.to_ascii_lowercase(), code]
            });
        let old_names = RULE_NAME_REDIRECTS.iter().map(|&(name, _)| name.to_owned());

        let mut sources = HashSet::new();
        for source in codes.chain(old_names) {
            assert!(
                sources.insert(source.clone()),
                "Duplicate redirect for `{source}`",
            );
        }
    }
}
