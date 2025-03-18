//! Remnant of the registry of all [`Rule`] implementations, now it's reexporting from codes.rs
//! with some helper symbols

use strum_macros::EnumIter;

pub use codes::Rule;
use ruff_macros::RuleNamespace;
pub use rule_set::{RuleSet, RuleSetIterator};

use crate::codes::{self};

mod rule_set;

pub trait AsRule {
    fn rule(&self) -> Rule;
}

impl Rule {
    pub fn from_code(code: &str) -> Result<Self, FromCodeError> {
        let (linter, code) = Linter::parse_code(code).ok_or(FromCodeError::Unknown)?;
        linter
            .all_rules()
            .find(|rule| rule.noqa_code().suffix() == code)
            .ok_or(FromCodeError::Unknown)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum FromCodeError {
    #[error("unknown rule code")]
    Unknown,
}

#[derive(EnumIter, Debug, PartialEq, Eq, Clone, Hash, RuleNamespace)]
pub enum Linter {
    /// [Airflow](https://pypi.org/project/apache-airflow/)
    #[prefix = "AIR"]
    Airflow,
    /// [eradicate](https://pypi.org/project/eradicate/)
    #[prefix = "ERA"]
    Eradicate,
    /// [FastAPI](https://pypi.org/project/fastapi/)
    #[prefix = "FAST"]
    FastApi,
    /// [flake8-2020](https://pypi.org/project/flake8-2020/)
    #[prefix = "YTT"]
    Flake82020,
    /// [flake8-annotations](https://pypi.org/project/flake8-annotations/)
    #[prefix = "ANN"]
    Flake8Annotations,
    /// [flake8-async](https://pypi.org/project/flake8-async/)
    #[prefix = "ASYNC"]
    Flake8Async,
    /// [flake8-bandit](https://pypi.org/project/flake8-bandit/)
    #[prefix = "S"]
    Flake8Bandit,
    /// [flake8-blind-except](https://pypi.org/project/flake8-blind-except/)
    #[prefix = "BLE"]
    Flake8BlindExcept,
    /// [flake8-boolean-trap](https://pypi.org/project/flake8-boolean-trap/)
    #[prefix = "FBT"]
    Flake8BooleanTrap,
    /// [flake8-bugbear](https://pypi.org/project/flake8-bugbear/)
    #[prefix = "B"]
    Flake8Bugbear,
    /// [flake8-builtins](https://pypi.org/project/flake8-builtins/)
    #[prefix = "A"]
    Flake8Builtins,
    /// [flake8-commas](https://pypi.org/project/flake8-commas/)
    #[prefix = "COM"]
    Flake8Commas,
    /// [flake8-comprehensions](https://pypi.org/project/flake8-comprehensions/)
    #[prefix = "C4"]
    Flake8Comprehensions,
    /// [flake8-copyright](https://pypi.org/project/flake8-copyright/)
    #[prefix = "CPY"]
    Flake8Copyright,
    /// [flake8-datetimez](https://pypi.org/project/flake8-datetimez/)
    #[prefix = "DTZ"]
    Flake8Datetimez,
    /// [flake8-debugger](https://pypi.org/project/flake8-debugger/)
    #[prefix = "T10"]
    Flake8Debugger,
    /// [flake8-django](https://pypi.org/project/flake8-django/)
    #[prefix = "DJ"]
    Flake8Django,
    /// [flake8-errmsg](https://pypi.org/project/flake8-errmsg/)
    #[prefix = "EM"]
    Flake8ErrMsg,
    /// [flake8-executable](https://pypi.org/project/flake8-executable/)
    #[prefix = "EXE"]
    Flake8Executable,
    /// [flake8-fixme](https://github.com/tommilligan/flake8-fixme)
    #[prefix = "FIX"]
    Flake8Fixme,
    /// [flake8-future-annotations](https://pypi.org/project/flake8-future-annotations/)
    #[prefix = "FA"]
    Flake8FutureAnnotations,
    /// [flake8-gettext](https://pypi.org/project/flake8-gettext/)
    #[prefix = "INT"]
    Flake8GetText,
    /// [flake8-implicit-str-concat](https://pypi.org/project/flake8-implicit-str-concat/)
    #[prefix = "ISC"]
    Flake8ImplicitStrConcat,
    /// [flake8-import-conventions](https://github.com/joaopalmeiro/flake8-import-conventions)
    #[prefix = "ICN"]
    Flake8ImportConventions,
    /// [flake8-logging](https://pypi.org/project/flake8-logging/)
    #[prefix = "LOG"]
    Flake8Logging,
    /// [flake8-logging-format](https://pypi.org/project/flake8-logging-format/)
    #[prefix = "G"]
    Flake8LoggingFormat,
    /// [flake8-no-pep420](https://pypi.org/project/flake8-no-pep420/)
    #[prefix = "INP"]
    Flake8NoPep420,
    /// [flake8-pie](https://pypi.org/project/flake8-pie/)
    #[prefix = "PIE"]
    Flake8Pie,
    /// [flake8-print](https://pypi.org/project/flake8-print/)
    #[prefix = "T20"]
    Flake8Print,
    /// [flake8-pyi](https://pypi.org/project/flake8-pyi/)
    #[prefix = "PYI"]
    Flake8Pyi,
    /// [flake8-pytest-style](https://pypi.org/project/flake8-pytest-style/)
    #[prefix = "PT"]
    Flake8PytestStyle,
    /// [flake8-quotes](https://pypi.org/project/flake8-quotes/)
    #[prefix = "Q"]
    Flake8Quotes,
    /// [flake8-raise](https://pypi.org/project/flake8-raise/)
    #[prefix = "RSE"]
    Flake8Raise,
    /// [flake8-return](https://pypi.org/project/flake8-return/)
    #[prefix = "RET"]
    Flake8Return,
    /// [flake8-self](https://pypi.org/project/flake8-self/)
    #[prefix = "SLF"]
    Flake8Self,
    /// [flake8-simplify](https://pypi.org/project/flake8-simplify/)
    #[prefix = "SIM"]
    Flake8Simplify,
    /// [flake8-slots](https://pypi.org/project/flake8-slots/)
    #[prefix = "SLOT"]
    Flake8Slots,
    /// [flake8-tidy-imports](https://pypi.org/project/flake8-tidy-imports/)
    #[prefix = "TID"]
    Flake8TidyImports,
    /// [flake8-todos](https://github.com/orsinium-labs/flake8-todos/)
    #[prefix = "TD"]
    Flake8Todos,
    /// [flake8-type-checking](https://pypi.org/project/flake8-type-checking/)
    #[prefix = "TC"]
    Flake8TypeChecking,
    /// [flake8-unused-arguments](https://pypi.org/project/flake8-unused-arguments/)
    #[prefix = "ARG"]
    Flake8UnusedArguments,
    /// [flake8-use-pathlib](https://pypi.org/project/flake8-use-pathlib/)
    #[prefix = "PTH"]
    Flake8UsePathlib,
    /// [flynt](https://pypi.org/project/flynt/)
    #[prefix = "FLY"]
    Flynt,
    /// [isort](https://pypi.org/project/isort/)
    #[prefix = "I"]
    Isort,
    /// [mccabe](https://pypi.org/project/mccabe/)
    #[prefix = "C90"]
    McCabe,
    /// NumPy-specific rules
    #[prefix = "NPY"]
    Numpy,
    /// [pandas-vet](https://pypi.org/project/pandas-vet/)
    #[prefix = "PD"]
    PandasVet,
    /// [pep8-naming](https://pypi.org/project/pep8-naming/)
    #[prefix = "N"]
    PEP8Naming,
    /// [Perflint](https://pypi.org/project/perflint/)
    #[prefix = "PERF"]
    Perflint,
    /// [pycodestyle](https://pypi.org/project/pycodestyle/)
    #[prefix = "E"]
    #[prefix = "W"]
    Pycodestyle,
    /// [pydoclint](https://pypi.org/project/pydoclint/)
    #[prefix = "DOC"]
    Pydoclint,
    /// [pydocstyle](https://pypi.org/project/pydocstyle/)
    #[prefix = "D"]
    Pydocstyle,
    /// [Pyflakes](https://pypi.org/project/pyflakes/)
    #[prefix = "F"]
    Pyflakes,
    /// [pygrep-hooks](https://github.com/pre-commit/pygrep-hooks)
    #[prefix = "PGH"]
    PygrepHooks,
    /// [Pylint](https://pypi.org/project/pylint/)
    #[prefix = "PL"]
    Pylint,
    /// [pyupgrade](https://pypi.org/project/pyupgrade/)
    #[prefix = "UP"]
    Pyupgrade,
    /// [refurb](https://pypi.org/project/refurb/)
    #[prefix = "FURB"]
    Refurb,
    /// Ruff-specific rules
    #[prefix = "RUF"]
    Ruff,
    /// [tryceratops](https://pypi.org/project/tryceratops/)
    #[prefix = "TRY"]
    Tryceratops,
}

pub trait RuleNamespace: Sized {
    /// Returns the prefix that every single code that ruff uses to identify
    /// rules from this linter starts with.  In the case that multiple
    /// `#[prefix]`es are configured for the variant in the `Linter` enum
    /// definition this is the empty string.
    fn common_prefix(&self) -> &'static str;

    /// Attempts to parse the given rule code. If the prefix is recognized
    /// returns the respective variant along with the code with the common
    /// prefix stripped.
    fn parse_code(code: &str) -> Option<(Self, &str)>;

    fn name(&self) -> &'static str;

    fn url(&self) -> Option<&'static str>;
}

#[derive(is_macro::Is, Copy, Clone)]
pub enum LintSource {
    Ast,
    Io,
    PhysicalLines,
    LogicalLines,
    Tokens,
    Imports,
    Noqa,
    Filesystem,
    PyprojectToml,
}

impl Rule {
    /// The source for the diagnostic (either the AST, the filesystem, or the
    /// physical lines).
    pub const fn lint_source(&self) -> LintSource {
        match self {
            Rule::InvalidPyprojectToml => LintSource::PyprojectToml,
            Rule::BlanketNOQA | Rule::RedirectedNOQA | Rule::UnusedNOQA => LintSource::Noqa,
            Rule::BidirectionalUnicode
            | Rule::BlankLineWithWhitespace
            | Rule::DocLineTooLong
            | Rule::IndentedFormFeed
            | Rule::LineTooLong
            | Rule::MissingCopyrightNotice
            | Rule::MissingNewlineAtEndOfFile
            | Rule::MixedSpacesAndTabs
            | Rule::TrailingWhitespace => LintSource::PhysicalLines,
            Rule::AmbiguousUnicodeCharacterComment
            | Rule::BlanketTypeIgnore
            | Rule::BlankLineAfterDecorator
            | Rule::BlankLineBetweenMethods
            | Rule::BlankLinesAfterFunctionOrClass
            | Rule::BlankLinesBeforeNestedDefinition
            | Rule::BlankLinesTopLevel
            | Rule::CommentedOutCode
            | Rule::EmptyComment
            | Rule::ExtraneousParentheses
            | Rule::InvalidCharacterBackspace
            | Rule::InvalidCharacterEsc
            | Rule::InvalidCharacterNul
            | Rule::InvalidCharacterSub
            | Rule::InvalidCharacterZeroWidthSpace
            | Rule::InvalidTodoCapitalization
            | Rule::InvalidTodoTag
            | Rule::LineContainsFixme
            | Rule::LineContainsHack
            | Rule::LineContainsTodo
            | Rule::LineContainsXxx
            | Rule::MissingSpaceAfterTodoColon
            | Rule::MissingTodoAuthor
            | Rule::MissingTodoColon
            | Rule::MissingTodoDescription
            | Rule::MissingTodoLink
            | Rule::MissingTrailingComma
            | Rule::MultiLineImplicitStringConcatenation
            | Rule::MultipleStatementsOnOneLineColon
            | Rule::MultipleStatementsOnOneLineSemicolon
            | Rule::ProhibitedTrailingComma
            | Rule::ShebangLeadingWhitespace
            | Rule::ShebangMissingExecutableFile
            | Rule::ShebangMissingPython
            | Rule::ShebangNotExecutable
            | Rule::ShebangNotFirstLine
            | Rule::SingleLineImplicitStringConcatenation
            | Rule::TabIndentation
            | Rule::TooManyBlankLines
            | Rule::TooManyNewlinesAtEndOfFile
            | Rule::TrailingCommaOnBareTuple
            | Rule::TypeCommentInStub
            | Rule::UselessSemicolon
            | Rule::UTF8EncodingDeclaration => LintSource::Tokens,
            Rule::IOError => LintSource::Io,
            Rule::UnsortedImports | Rule::MissingRequiredImport => LintSource::Imports,
            Rule::ImplicitNamespacePackage
            | Rule::InvalidModuleName
            | Rule::StdlibModuleShadowing => LintSource::Filesystem,
            Rule::IndentationWithInvalidMultiple
            | Rule::IndentationWithInvalidMultipleComment
            | Rule::MissingWhitespace
            | Rule::MissingWhitespaceAfterKeyword
            | Rule::MissingWhitespaceAroundArithmeticOperator
            | Rule::MissingWhitespaceAroundBitwiseOrShiftOperator
            | Rule::MissingWhitespaceAroundModuloOperator
            | Rule::MissingWhitespaceAroundOperator
            | Rule::MissingWhitespaceAroundParameterEquals
            | Rule::MultipleLeadingHashesForBlockComment
            | Rule::MultipleSpacesAfterComma
            | Rule::MultipleSpacesAfterKeyword
            | Rule::MultipleSpacesAfterOperator
            | Rule::MultipleSpacesBeforeKeyword
            | Rule::MultipleSpacesBeforeOperator
            | Rule::NoIndentedBlock
            | Rule::NoIndentedBlockComment
            | Rule::NoSpaceAfterBlockComment
            | Rule::NoSpaceAfterInlineComment
            | Rule::OverIndented
            | Rule::RedundantBackslash
            | Rule::TabAfterComma
            | Rule::TabAfterKeyword
            | Rule::TabAfterOperator
            | Rule::TabBeforeKeyword
            | Rule::TabBeforeOperator
            | Rule::TooFewSpacesBeforeInlineComment
            | Rule::UnexpectedIndentation
            | Rule::UnexpectedIndentationComment
            | Rule::UnexpectedSpacesAroundKeywordParameterEquals
            | Rule::WhitespaceAfterOpenBracket
            | Rule::WhitespaceBeforeCloseBracket
            | Rule::WhitespaceBeforeParameters
            | Rule::WhitespaceBeforePunctuation => LintSource::LogicalLines,
            _ => LintSource::Ast,
        }
    }

    /// Return the URL for the rule documentation, if it exists.
    pub fn url(&self) -> Option<String> {
        self.explanation()
            .is_some()
            .then(|| format!("{}/rules/{}", env!("CARGO_PKG_HOMEPAGE"), self.as_ref()))
    }
}

/// Pairs of checks that shouldn't be enabled together.
pub const INCOMPATIBLE_CODES: &[(Rule, Rule, &str); 2] = &[
    (
        Rule::BlankLineBeforeClass,
        Rule::IncorrectBlankLineBeforeClass,
        "`incorrect-blank-line-before-class` (D203) and `no-blank-line-before-class` (D211) are \
         incompatible. Ignoring `incorrect-blank-line-before-class`.",
    ),
    (
        Rule::MultiLineSummaryFirstLine,
        Rule::MultiLineSummarySecondLine,
        "`multi-line-summary-first-line` (D212) and `multi-line-summary-second-line` (D213) are \
         incompatible. Ignoring `multi-line-summary-second-line`.",
    ),
];

#[cfg(feature = "clap")]
pub mod clap_completion {
    use clap::builder::{PossibleValue, TypedValueParser, ValueParserFactory};
    use strum::IntoEnumIterator;

    use crate::registry::Rule;

    #[derive(Clone)]
    pub struct RuleParser;

    impl ValueParserFactory for Rule {
        type Parser = RuleParser;

        fn value_parser() -> Self::Parser {
            RuleParser
        }
    }

    impl TypedValueParser for RuleParser {
        type Value = Rule;

        fn parse_ref(
            &self,
            cmd: &clap::Command,
            arg: Option<&clap::Arg>,
            value: &std::ffi::OsStr,
        ) -> Result<Self::Value, clap::Error> {
            let value = value
                .to_str()
                .ok_or_else(|| clap::Error::new(clap::error::ErrorKind::InvalidUtf8))?;

            Rule::from_code(value).map_err(|_| {
                let mut error =
                    clap::Error::new(clap::error::ErrorKind::ValueValidation).with_cmd(cmd);
                if let Some(arg) = arg {
                    error.insert(
                        clap::error::ContextKind::InvalidArg,
                        clap::error::ContextValue::String(arg.to_string()),
                    );
                }
                error.insert(
                    clap::error::ContextKind::InvalidValue,
                    clap::error::ContextValue::String(value.to_string()),
                );
                error
            })
        }

        fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
            Some(Box::new(Rule::iter().map(|rule| {
                let name = rule.noqa_code().to_string();
                let help = rule.as_ref().to_string();
                PossibleValue::new(name).help(help)
            })))
        }
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;
    use std::mem::size_of;

    use strum::IntoEnumIterator;

    use super::{Linter, Rule, RuleNamespace};

    #[test]
    fn documentation() {
        for rule in Rule::iter() {
            assert!(
                rule.explanation().is_some(),
                "Rule {} is missing documentation",
                rule.as_ref()
            );
        }
    }

    #[test]
    fn rule_naming_convention() {
        // The disallowed rule names are defined in a separate file so that they can also be picked up by add_rule.py.
        let patterns: Vec<_> = include_str!("../resources/test/disallowed_rule_names.txt")
            .trim()
            .split('\n')
            .map(|line| {
                glob::Pattern::new(line).expect("malformed pattern in disallowed_rule_names.txt")
            })
            .collect();

        for rule in Rule::iter() {
            let rule_name = rule.as_ref();
            for pattern in &patterns {
                assert!(
                    !pattern.matches(rule_name),
                    "{rule_name} does not match naming convention, see CONTRIBUTING.md"
                );
            }
        }
    }

    #[test]
    fn check_code_serialization() {
        for rule in Rule::iter() {
            assert!(
                Rule::from_code(&format!("{}", rule.noqa_code())).is_ok(),
                "{rule:?} could not be round-trip serialized."
            );
        }
    }

    #[test]
    fn linter_parse_code() {
        for rule in Rule::iter() {
            let code = format!("{}", rule.noqa_code());
            let (linter, rest) =
                Linter::parse_code(&code).unwrap_or_else(|| panic!("couldn't parse {code:?}"));
            assert_eq!(code, format!("{}{rest}", linter.common_prefix()));
        }
    }

    #[test]
    fn rule_size() {
        assert_eq!(2, size_of::<Rule>());
    }

    #[test]
    fn linter_sorting() {
        let names: Vec<_> = Linter::iter()
            .map(|linter| linter.name().to_lowercase())
            .collect();

        let sorted: Vec<_> = names.iter().cloned().sorted().collect();

        assert_eq!(
            &names[..],
            &sorted[..],
            "Linters are not sorted alphabetically (case insensitive)"
        );
    }
}
