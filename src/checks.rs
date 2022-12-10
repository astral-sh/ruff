use std::fmt;
use std::str::FromStr;

use itertools::Itertools;
use once_cell::sync::Lazy;
use rustc_hash::FxHashMap;
use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, Display, EnumIter, EnumString};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checks_gen::CheckCodePrefix;
use crate::flake8_debugger::types::DebuggerUsingType;
use crate::flake8_quotes::settings::Quote;
use crate::flake8_tidy_imports::settings::Strictness;
use crate::pyupgrade::types::Primitive;

#[derive(
    AsRefStr,
    EnumIter,
    EnumString,
    Debug,
    Display,
    PartialEq,
    Eq,
    Clone,
    Serialize,
    Deserialize,
    Hash,
    PartialOrd,
    Ord,
)]
pub enum CheckCode {
    // pycodestyle errors
    E402,
    E501,
    E711,
    E712,
    E713,
    E714,
    E721,
    E722,
    E731,
    E741,
    E742,
    E743,
    E902,
    E999,
    // pycodestyle warnings
    W292,
    W605,
    // pyflakes
    F401,
    F402,
    F403,
    F404,
    F405,
    F406,
    F407,
    F501,
    F502,
    F503,
    F504,
    F505,
    F506,
    F507,
    F508,
    F509,
    F521,
    F522,
    F523,
    F524,
    F525,
    F541,
    F601,
    F602,
    F621,
    F622,
    F631,
    F632,
    F633,
    F634,
    F701,
    F702,
    F704,
    F706,
    F707,
    F722,
    F811,
    F821,
    F822,
    F823,
    F831,
    F841,
    F842,
    F901,
    // pylint
    PLC0414,
    PLC2201,
    PLC3002,
    PLE0117,
    PLE0118,
    PLE1142,
    PLR0206,
    PLR0402,
    PLR1701,
    PLR1722,
    PLW0120,
    PLW0602,
    // flake8-builtins
    A001,
    A002,
    A003,
    // flake8-bugbear
    B002,
    B003,
    B004,
    B005,
    B006,
    B007,
    B008,
    B009,
    B010,
    B011,
    B012,
    B013,
    B014,
    B015,
    B016,
    B017,
    B018,
    B019,
    B020,
    B021,
    B022,
    B023,
    B024,
    B025,
    B026,
    B027,
    B904,
    B905,
    // flake8-blind-except
    BLE001,
    // flake8-comprehensions
    C400,
    C401,
    C402,
    C403,
    C404,
    C405,
    C406,
    C408,
    C409,
    C410,
    C411,
    C413,
    C414,
    C415,
    C416,
    C417,
    // flake8-debugger
    T100,
    // mccabe
    C901,
    // flake8-tidy-imports
    TID252,
    // flake8-return
    RET501,
    RET502,
    RET503,
    RET504,
    RET505,
    RET506,
    RET507,
    RET508,
    // flake8-print
    T201,
    T203,
    // flake8-quotes
    Q000,
    Q001,
    Q002,
    Q003,
    // flake8-annotations
    ANN001,
    ANN002,
    ANN003,
    ANN101,
    ANN102,
    ANN201,
    ANN202,
    ANN204,
    ANN205,
    ANN206,
    ANN401,
    // flake8-2020
    YTT101,
    YTT102,
    YTT103,
    YTT201,
    YTT202,
    YTT203,
    YTT204,
    YTT301,
    YTT302,
    YTT303,
    // pyupgrade
    UP001,
    UP003,
    UP004,
    UP005,
    UP006,
    UP007,
    UP008,
    UP009,
    UP010,
    UP011,
    UP012,
    UP013,
    UP014,
    UP015,
    // pydocstyle
    D100,
    D101,
    D102,
    D103,
    D104,
    D105,
    D106,
    D107,
    D200,
    D201,
    D202,
    D203,
    D204,
    D205,
    D206,
    D207,
    D208,
    D209,
    D210,
    D211,
    D212,
    D213,
    D214,
    D215,
    D300,
    D301,
    D400,
    D402,
    D403,
    D404,
    D405,
    D406,
    D407,
    D408,
    D409,
    D410,
    D411,
    D412,
    D413,
    D414,
    D415,
    D416,
    D417,
    D418,
    D419,
    // pep8-naming
    N801,
    N802,
    N803,
    N804,
    N805,
    N806,
    N807,
    N811,
    N812,
    N813,
    N814,
    N815,
    N816,
    N817,
    N818,
    // isort
    I001,
    // eradicate
    ERA001,
    // flake8-bandit
    S101,
    S102,
    S104,
    S105,
    S106,
    S107,
    // flake8-boolean-trap
    FBT001,
    FBT002,
    FBT003,
    // flake8-unused-arguments
    ARG001,
    ARG002,
    ARG003,
    ARG004,
    ARG005,
    // flake8-import-conventions
    ICN001,
    // Ruff
    RUF001,
    RUF002,
    RUF003,
    RUF100,
    // pygrep-hooks
    PGH001,
}

#[derive(EnumIter, Debug, PartialEq, Eq)]
pub enum CheckCategory {
    Pyflakes,
    Pycodestyle,
    McCabe,
    Isort,
    Pydocstyle,
    Pyupgrade,
    PEP8Naming,
    Flake82020,
    Flake8Annotations,
    Flake8Bandit,
    Flake8BlindExcept,
    Flake8BooleanTrap,
    Flake8Bugbear,
    Flake8Builtins,
    Flake8Comprehensions,
    Flake8Debugger,
    Flake8ImportConventions,
    Flake8Print,
    Flake8Quotes,
    Flake8Return,
    Flake8TidyImports,
    Flake8UnusedArguments,
    Eradicate,
    PygrepHooks,
    Pylint,
    Ruff,
}

pub enum Platform {
    PyPI,
    GitHub,
}

impl fmt::Display for Platform {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Platform::PyPI => fmt.write_str("PyPI"),
            Platform::GitHub => fmt.write_str("GitHub"),
        }
    }
}

impl CheckCategory {
    pub fn title(&self) -> &'static str {
        match self {
            CheckCategory::Eradicate => "eradicate",
            CheckCategory::Flake82020 => "flake8-2020",
            CheckCategory::Flake8Annotations => "flake8-annotations",
            CheckCategory::Flake8Bandit => "flake8-bandit",
            CheckCategory::Flake8BlindExcept => "flake8-blind-except",
            CheckCategory::Flake8BooleanTrap => "flake8-boolean-trap",
            CheckCategory::Flake8Bugbear => "flake8-bugbear",
            CheckCategory::Flake8Builtins => "flake8-builtins",
            CheckCategory::Flake8Comprehensions => "flake8-comprehensions",
            CheckCategory::Flake8Debugger => "flake8-debugger",
            CheckCategory::Flake8ImportConventions => "flake8-import-conventions",
            CheckCategory::Flake8Print => "flake8-print",
            CheckCategory::Flake8Quotes => "flake8-quotes",
            CheckCategory::Flake8Return => "flake8-return",
            CheckCategory::Flake8TidyImports => "flake8-tidy-imports",
            CheckCategory::Flake8UnusedArguments => "flake8-unused-arguments",
            CheckCategory::Isort => "isort",
            CheckCategory::McCabe => "mccabe",
            CheckCategory::PEP8Naming => "pep8-naming",
            CheckCategory::Pycodestyle => "pycodestyle",
            CheckCategory::Pydocstyle => "pydocstyle",
            CheckCategory::Pyflakes => "Pyflakes",
            CheckCategory::PygrepHooks => "pygrep-hooks",
            CheckCategory::Pylint => "Pylint",
            CheckCategory::Pyupgrade => "pyupgrade",
            CheckCategory::Ruff => "Ruff-specific rules",
        }
    }

    pub fn codes(&self) -> Vec<CheckCodePrefix> {
        match self {
            CheckCategory::Eradicate => vec![CheckCodePrefix::ERA],
            CheckCategory::Flake82020 => vec![CheckCodePrefix::YTT],
            CheckCategory::Flake8Annotations => vec![CheckCodePrefix::ANN],
            CheckCategory::Flake8Bandit => vec![CheckCodePrefix::S],
            CheckCategory::Flake8BlindExcept => vec![CheckCodePrefix::BLE],
            CheckCategory::Flake8BooleanTrap => vec![CheckCodePrefix::FBT],
            CheckCategory::Flake8Bugbear => vec![CheckCodePrefix::B],
            CheckCategory::Flake8Builtins => vec![CheckCodePrefix::A],
            CheckCategory::Flake8Comprehensions => vec![CheckCodePrefix::C4],
            CheckCategory::Flake8Debugger => vec![CheckCodePrefix::T10],
            CheckCategory::Flake8Print => vec![CheckCodePrefix::T20],
            CheckCategory::Flake8Quotes => vec![CheckCodePrefix::Q],
            CheckCategory::Flake8Return => vec![CheckCodePrefix::RET],
            CheckCategory::Flake8TidyImports => vec![CheckCodePrefix::TID],
            CheckCategory::Flake8UnusedArguments => vec![CheckCodePrefix::ARG],
            CheckCategory::Isort => vec![CheckCodePrefix::I],
            CheckCategory::McCabe => vec![CheckCodePrefix::C90],
            CheckCategory::PEP8Naming => vec![CheckCodePrefix::N],
            CheckCategory::Pycodestyle => vec![CheckCodePrefix::E, CheckCodePrefix::W],
            CheckCategory::Pydocstyle => vec![CheckCodePrefix::D],
            CheckCategory::Pyflakes => vec![CheckCodePrefix::F],
            CheckCategory::PygrepHooks => vec![CheckCodePrefix::PGH],
            CheckCategory::Pylint => vec![
                CheckCodePrefix::PLC,
                CheckCodePrefix::PLE,
                CheckCodePrefix::PLR,
                CheckCodePrefix::PLW,
            ],
            CheckCategory::Pyupgrade => vec![CheckCodePrefix::UP],
            CheckCategory::Flake8ImportConventions => vec![CheckCodePrefix::ICN],
            CheckCategory::Ruff => vec![CheckCodePrefix::RUF],
        }
    }

    pub fn url(&self) -> Option<(&'static str, &'static Platform)> {
        match self {
            CheckCategory::Eradicate => {
                Some(("https://pypi.org/project/eradicate/2.1.0/", &Platform::PyPI))
            }
            CheckCategory::Flake82020 => Some((
                "https://pypi.org/project/flake8-2020/1.7.0/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8Annotations => Some((
                "https://pypi.org/project/flake8-annotations/2.9.1/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8Bandit => Some((
                "https://pypi.org/project/flake8-bandit/4.1.1/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8BlindExcept => Some((
                "https://pypi.org/project/flake8-blind-except/0.2.1/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8BooleanTrap => Some((
                "https://pypi.org/project/flake8-boolean-trap/0.1.0/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8Bugbear => Some((
                "https://pypi.org/project/flake8-bugbear/22.10.27/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8Builtins => Some((
                "https://pypi.org/project/flake8-builtins/2.0.1/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8Comprehensions => Some((
                "https://pypi.org/project/flake8-comprehensions/3.10.1/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8Debugger => Some((
                "https://pypi.org/project/flake8-debugger/4.1.2/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8ImportConventions => None,
            CheckCategory::Flake8Print => Some((
                "https://pypi.org/project/flake8-print/5.0.0/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8Quotes => Some((
                "https://pypi.org/project/flake8-quotes/3.3.1/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8Return => Some((
                "https://pypi.org/project/flake8-return/1.2.0/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8TidyImports => Some((
                "https://pypi.org/project/flake8-tidy-imports/4.8.0/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8UnusedArguments => Some((
                "https://pypi.org/project/flake8-unused-arguments/0.0.12/",
                &Platform::PyPI,
            )),
            CheckCategory::Isort => {
                Some(("https://pypi.org/project/isort/5.10.1/", &Platform::PyPI))
            }
            CheckCategory::McCabe => {
                Some(("https://pypi.org/project/mccabe/0.7.0/", &Platform::PyPI))
            }
            CheckCategory::PEP8Naming => Some((
                "https://pypi.org/project/pep8-naming/0.13.2/",
                &Platform::PyPI,
            )),
            CheckCategory::Pycodestyle => Some((
                "https://pypi.org/project/pycodestyle/2.9.1/",
                &Platform::PyPI,
            )),
            CheckCategory::Pydocstyle => Some((
                "https://pypi.org/project/pydocstyle/6.1.1/",
                &Platform::PyPI,
            )),
            CheckCategory::Pyflakes => {
                Some(("https://pypi.org/project/pyflakes/2.5.0/", &Platform::PyPI))
            }
            CheckCategory::Pylint => {
                Some(("https://pypi.org/project/pylint/2.15.7/", &Platform::PyPI))
            }
            CheckCategory::PygrepHooks => Some((
                "https://github.com/pre-commit/pygrep-hooks",
                &Platform::GitHub,
            )),
            CheckCategory::Pyupgrade => {
                Some(("https://pypi.org/project/pyupgrade/3.2.0/", &Platform::PyPI))
            }
            CheckCategory::Ruff => None,
        }
    }
}

#[allow(clippy::upper_case_acronyms)]
pub enum LintSource {
    AST,
    FileSystem,
    Lines,
    Tokens,
    Imports,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectedCmpop {
    Eq,
    NotEq,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeferralKeyword {
    Yield,
    YieldFrom,
    Await,
}

impl fmt::Display for DeferralKeyword {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DeferralKeyword::Yield => fmt.write_str("yield"),
            DeferralKeyword::YieldFrom => fmt.write_str("yield from"),
            DeferralKeyword::Await => fmt.write_str("await"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Branch {
    Elif,
    Else,
}

impl fmt::Display for Branch {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Branch::Elif => fmt.write_str("elif"),
            Branch::Else => fmt.write_str("else"),
        }
    }
}

#[derive(AsRefStr, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckKind {
    // pycodestyle errors
    AmbiguousClassName(String),
    AmbiguousFunctionName(String),
    AmbiguousVariableName(String),
    DoNotAssignLambda,
    DoNotUseBareExcept,
    IOError(String),
    LineTooLong(usize, usize),
    ModuleImportNotAtTopOfFile,
    NoneComparison(RejectedCmpop),
    NotInTest,
    NotIsTest,
    SyntaxError(String),
    TrueFalseComparison(bool, RejectedCmpop),
    TypeComparison,
    // pycodestyle warnings
    NoNewLineAtEndOfFile,
    InvalidEscapeSequence(char),
    // pyflakes
    AssertTuple,
    BreakOutsideLoop,
    ContinueOutsideLoop,
    DefaultExceptNotLast,
    DuplicateArgumentName,
    ExpressionsInStarAssignment,
    FStringMissingPlaceholders,
    ForwardAnnotationSyntaxError(String),
    FutureFeatureNotDefined(String),
    IfTuple,
    ImportShadowedByLoopVar(String, usize),
    ImportStarNotPermitted(String),
    ImportStarUsage(String, Vec<String>),
    ImportStarUsed(String),
    InvalidPrintSyntax,
    IsLiteral,
    LateFutureImport,
    MultiValueRepeatedKeyLiteral,
    MultiValueRepeatedKeyVariable(String),
    PercentFormatExpectedMapping,
    PercentFormatExpectedSequence,
    PercentFormatExtraNamedArguments(Vec<String>),
    PercentFormatInvalidFormat(String),
    PercentFormatMissingArgument(Vec<String>),
    PercentFormatMixedPositionalAndNamed,
    PercentFormatPositionalCountMismatch(usize, usize),
    PercentFormatStarRequiresSequence,
    PercentFormatUnsupportedFormatCharacter(char),
    RaiseNotImplemented,
    RedefinedWhileUnused(String, usize),
    ReturnOutsideFunction,
    StringDotFormatExtraNamedArguments(Vec<String>),
    StringDotFormatExtraPositionalArguments(Vec<String>),
    StringDotFormatInvalidFormat(String),
    StringDotFormatMissingArguments(Vec<String>),
    StringDotFormatMixingAutomatic,
    TwoStarredExpressions,
    UndefinedExport(String),
    UndefinedLocal(String),
    UnusedAnnotation(String),
    UndefinedName(String),
    UnusedImport(String, bool),
    UnusedVariable(String),
    YieldOutsideFunction(DeferralKeyword),
    // pylint
    AwaitOutsideAsync,
    ConsiderMergingIsinstance(String, Vec<String>),
    ConsiderUsingFromImport(String, String),
    GlobalVariableNotAssigned(String),
    MisplacedComparisonConstant(String),
    NonlocalWithoutBinding(String),
    PropertyWithParameters,
    UnnecessaryDirectLambdaCall,
    UseSysExit(String),
    UsedPriorGlobalDeclaration(String, usize),
    UselessElseOnLoop,
    UselessImportAlias,
    // flake8-builtins
    BuiltinVariableShadowing(String),
    BuiltinArgumentShadowing(String),
    BuiltinAttributeShadowing(String),
    // flake8-blind-except
    BlindExcept(String),
    // flake8-bugbear
    AbstractBaseClassWithoutAbstractMethod(String),
    AssignmentToOsEnviron,
    CachedInstanceMethod,
    CannotRaiseLiteral,
    DoNotAssertFalse,
    DuplicateHandlerException(Vec<String>),
    DuplicateTryBlockException(String),
    EmptyMethodWithoutAbstractDecorator(String),
    FStringDocstring,
    FunctionCallArgumentDefault(Option<String>),
    FunctionUsesLoopVariable(String),
    GetAttrWithConstant,
    JumpStatementInFinally(String),
    LoopVariableOverridesIterator(String),
    MutableArgumentDefault,
    NoAssertRaisesException,
    RaiseWithoutFromInsideExcept,
    RedundantTupleInExceptionHandler(String),
    SetAttrWithConstant,
    StarArgUnpackingAfterKeywordArg,
    StripWithMultiCharacters,
    UnaryPrefixIncrement,
    UnreliableCallableCheck,
    UnusedLoopControlVariable(String),
    UselessComparison,
    UselessContextlibSuppress,
    UselessExpression,
    ZipWithoutExplicitStrict,
    // flake8-comprehensions
    UnnecessaryGeneratorList,
    UnnecessaryGeneratorSet,
    UnnecessaryGeneratorDict,
    UnnecessaryListComprehensionSet,
    UnnecessaryListComprehensionDict,
    UnnecessaryLiteralSet(String),
    UnnecessaryLiteralDict(String),
    UnnecessaryCollectionCall(String),
    UnnecessaryLiteralWithinTupleCall(String),
    UnnecessaryLiteralWithinListCall(String),
    UnnecessaryListCall,
    UnnecessaryCallAroundSorted(String),
    UnnecessaryDoubleCastOrProcess(String, String),
    UnnecessarySubscriptReversal(String),
    UnnecessaryComprehension(String),
    UnnecessaryMap(String),
    // flake8-debugger
    Debugger(DebuggerUsingType),
    // flake8-tidy-imports
    BannedRelativeImport(Strictness),
    // flake8-return
    UnnecessaryReturnNone,
    ImplicitReturnValue,
    ImplicitReturn,
    UnnecessaryAssign,
    SuperfluousElseReturn(Branch),
    SuperfluousElseRaise(Branch),
    SuperfluousElseContinue(Branch),
    SuperfluousElseBreak(Branch),
    // flake8-print
    PrintFound,
    PPrintFound,
    // flake8-quotes
    BadQuotesInlineString(Quote),
    BadQuotesMultilineString(Quote),
    BadQuotesDocstring(Quote),
    AvoidQuoteEscape,
    // flake8-annotations
    MissingTypeFunctionArgument(String),
    MissingTypeArgs(String),
    MissingTypeKwargs(String),
    MissingTypeSelf(String),
    MissingTypeCls(String),
    MissingReturnTypePublicFunction(String),
    MissingReturnTypePrivateFunction(String),
    MissingReturnTypeMagicMethod(String),
    MissingReturnTypeStaticMethod(String),
    MissingReturnTypeClassMethod(String),
    DynamicallyTypedExpression(String),
    // flake8-2020
    SysVersionSlice3Referenced,
    SysVersion2Referenced,
    SysVersionCmpStr3,
    SysVersionInfo0Eq3Referenced,
    SixPY3Referenced,
    SysVersionInfo1CmpInt,
    SysVersionInfoMinorCmpInt,
    SysVersion0Referenced,
    SysVersionCmpStr10,
    SysVersionSlice1Referenced,
    // pyupgrade
    TypeOfPrimitive(Primitive),
    UselessMetaclassType,
    DeprecatedUnittestAlias(String, String),
    UselessObjectInheritance(String),
    UsePEP585Annotation(String),
    UsePEP604Annotation,
    SuperCallWithParameters,
    PEP3120UnnecessaryCodingComment,
    UnnecessaryFutureImport(Vec<String>),
    UnnecessaryLRUCacheParams,
    UnnecessaryEncodeUTF8,
    ConvertTypedDictFunctionalToClass(String),
    ConvertNamedTupleFunctionalToClass(String),
    RedundantOpenModes,
    // pydocstyle
    BlankLineAfterLastSection(String),
    BlankLineAfterSection(String),
    BlankLineAfterSummary,
    BlankLineBeforeSection(String),
    CapitalizeSectionName(String),
    DashedUnderlineAfterSection(String),
    DocumentAllArguments(Vec<String>),
    EndsInPeriod,
    EndsInPunctuation,
    FirstLineCapitalized,
    FitsOnOneLine,
    IndentWithSpaces,
    MagicMethod,
    MultiLineSummaryFirstLine,
    MultiLineSummarySecondLine,
    NewLineAfterLastParagraph,
    NewLineAfterSectionName(String),
    NoBlankLineAfterFunction(usize),
    NoBlankLineBeforeClass(usize),
    NoBlankLineBeforeFunction(usize),
    NoBlankLinesBetweenHeaderAndContent(String),
    NoOverIndentation,
    NoSignature,
    NoSurroundingWhitespace,
    NoThisPrefix,
    NoUnderIndentation,
    NonEmpty,
    NonEmptySection(String),
    OneBlankLineAfterClass(usize),
    OneBlankLineBeforeClass(usize),
    PublicClass,
    PublicFunction,
    PublicInit,
    PublicMethod,
    PublicModule,
    PublicNestedClass,
    PublicPackage,
    SectionNameEndsInColon(String),
    SectionNotOverIndented(String),
    SectionUnderlineAfterName(String),
    SectionUnderlineMatchesSectionLength(String),
    SectionUnderlineNotOverIndented(String),
    SkipDocstring,
    UsesRPrefixForBackslashedContent,
    UsesTripleQuotes,
    // pep8-naming
    InvalidClassName(String),
    InvalidFunctionName(String),
    InvalidArgumentName(String),
    InvalidFirstArgumentNameForClassMethod,
    InvalidFirstArgumentNameForMethod,
    NonLowercaseVariableInFunction(String),
    DunderFunctionName,
    ConstantImportedAsNonConstant(String, String),
    LowercaseImportedAsNonLowercase(String, String),
    CamelcaseImportedAsLowercase(String, String),
    CamelcaseImportedAsConstant(String, String),
    MixedCaseVariableInClassScope(String),
    MixedCaseVariableInGlobalScope(String),
    CamelcaseImportedAsAcronym(String, String),
    ErrorSuffixOnExceptionName(String),
    // isort
    UnsortedImports,
    // eradicate
    CommentedOutCode,
    // flake8-bandit
    AssertUsed,
    ExecUsed,
    HardcodedBindAllInterfaces,
    HardcodedPasswordString(String),
    HardcodedPasswordFuncArg(String),
    HardcodedPasswordDefault(String),
    // mccabe
    FunctionIsTooComplex(String, usize),
    // flake8-boolean-trap
    BooleanPositionalArgInFunctionDefinition,
    BooleanDefaultValueInFunctionDefinition,
    BooleanPositionalValueInFunctionCall,
    // pygrep-hooks
    NoEval,
    // flake8-unused-arguments
    UnusedFunctionArgument(String),
    UnusedMethodArgument(String),
    UnusedClassMethodArgument(String),
    UnusedStaticMethodArgument(String),
    UnusedLambdaArgument(String),
    // flake8-import-conventions
    ImportAliasIsNotConventional(String, String),
    // Ruff
    AmbiguousUnicodeCharacterString(char, char),
    AmbiguousUnicodeCharacterDocstring(char, char),
    AmbiguousUnicodeCharacterComment(char, char),
    UnusedNOQA(Option<Vec<String>>),
}

impl CheckCode {
    /// The source for the check (either the AST, the filesystem, or the
    /// physical lines).
    pub fn lint_source(&self) -> &'static LintSource {
        match self {
            CheckCode::E501 | CheckCode::W292 | CheckCode::RUF100 | CheckCode::UP009 => {
                &LintSource::Lines
            }
            CheckCode::ERA001
            | CheckCode::Q000
            | CheckCode::Q001
            | CheckCode::Q002
            | CheckCode::Q003
            | CheckCode::W605
            | CheckCode::RUF001
            | CheckCode::RUF002
            | CheckCode::RUF003 => &LintSource::Tokens,
            CheckCode::E902 => &LintSource::FileSystem,
            CheckCode::I001 => &LintSource::Imports,
            _ => &LintSource::AST,
        }
    }

    /// A placeholder representation of the `CheckKind` for the check.
    pub fn kind(&self) -> CheckKind {
        match self {
            // pycodestyle errors
            CheckCode::E402 => CheckKind::ModuleImportNotAtTopOfFile,
            CheckCode::E501 => CheckKind::LineTooLong(89, 88),
            CheckCode::E711 => CheckKind::NoneComparison(RejectedCmpop::Eq),
            CheckCode::E712 => CheckKind::TrueFalseComparison(true, RejectedCmpop::Eq),
            CheckCode::E713 => CheckKind::NotInTest,
            CheckCode::E714 => CheckKind::NotIsTest,
            CheckCode::E721 => CheckKind::TypeComparison,
            CheckCode::E722 => CheckKind::DoNotUseBareExcept,
            CheckCode::E731 => CheckKind::DoNotAssignLambda,
            CheckCode::E741 => CheckKind::AmbiguousVariableName("...".to_string()),
            CheckCode::E742 => CheckKind::AmbiguousClassName("...".to_string()),
            CheckCode::E743 => CheckKind::AmbiguousFunctionName("...".to_string()),
            CheckCode::E902 => CheckKind::IOError("IOError: `...`".to_string()),
            CheckCode::E999 => CheckKind::SyntaxError("`...`".to_string()),
            // pycodestyle warnings
            CheckCode::W292 => CheckKind::NoNewLineAtEndOfFile,
            CheckCode::W605 => CheckKind::InvalidEscapeSequence('c'),
            // pyflakes
            CheckCode::F401 => CheckKind::UnusedImport("...".to_string(), false),
            CheckCode::F402 => CheckKind::ImportShadowedByLoopVar("...".to_string(), 1),
            CheckCode::F403 => CheckKind::ImportStarUsed("...".to_string()),
            CheckCode::F404 => CheckKind::LateFutureImport,
            CheckCode::F405 => {
                CheckKind::ImportStarUsage("...".to_string(), vec!["...".to_string()])
            }
            CheckCode::F406 => CheckKind::ImportStarNotPermitted("...".to_string()),
            CheckCode::F407 => CheckKind::FutureFeatureNotDefined("...".to_string()),
            CheckCode::F501 => CheckKind::PercentFormatInvalidFormat("...".to_string()),
            CheckCode::F502 => CheckKind::PercentFormatExpectedMapping,
            CheckCode::F503 => CheckKind::PercentFormatExpectedSequence,
            CheckCode::F504 => CheckKind::PercentFormatExtraNamedArguments(vec!["...".to_string()]),
            CheckCode::F505 => CheckKind::PercentFormatMissingArgument(vec!["...".to_string()]),
            CheckCode::F506 => CheckKind::PercentFormatMixedPositionalAndNamed,
            CheckCode::F507 => CheckKind::PercentFormatPositionalCountMismatch(4, 2),
            CheckCode::F508 => CheckKind::PercentFormatStarRequiresSequence,
            CheckCode::F509 => CheckKind::PercentFormatUnsupportedFormatCharacter('c'),
            CheckCode::F521 => CheckKind::StringDotFormatInvalidFormat("...".to_string()),
            CheckCode::F522 => {
                CheckKind::StringDotFormatExtraNamedArguments(vec!["...".to_string()])
            }
            CheckCode::F523 => {
                CheckKind::StringDotFormatExtraPositionalArguments(vec!["...".to_string()])
            }
            CheckCode::F524 => CheckKind::StringDotFormatMissingArguments(vec!["...".to_string()]),
            CheckCode::F525 => CheckKind::StringDotFormatMixingAutomatic,
            CheckCode::F541 => CheckKind::FStringMissingPlaceholders,
            CheckCode::F601 => CheckKind::MultiValueRepeatedKeyLiteral,
            CheckCode::F602 => CheckKind::MultiValueRepeatedKeyVariable("...".to_string()),
            CheckCode::F621 => CheckKind::ExpressionsInStarAssignment,
            CheckCode::F622 => CheckKind::TwoStarredExpressions,
            CheckCode::F631 => CheckKind::AssertTuple,
            CheckCode::F632 => CheckKind::IsLiteral,
            CheckCode::F633 => CheckKind::InvalidPrintSyntax,
            CheckCode::F634 => CheckKind::IfTuple,
            CheckCode::F701 => CheckKind::BreakOutsideLoop,
            CheckCode::F702 => CheckKind::ContinueOutsideLoop,
            CheckCode::F704 => CheckKind::YieldOutsideFunction(DeferralKeyword::Yield),
            CheckCode::F706 => CheckKind::ReturnOutsideFunction,
            CheckCode::F707 => CheckKind::DefaultExceptNotLast,
            CheckCode::F722 => CheckKind::ForwardAnnotationSyntaxError("...".to_string()),
            CheckCode::F811 => CheckKind::RedefinedWhileUnused("...".to_string(), 1),
            CheckCode::F821 => CheckKind::UndefinedName("...".to_string()),
            CheckCode::F822 => CheckKind::UndefinedExport("...".to_string()),
            CheckCode::F823 => CheckKind::UndefinedLocal("...".to_string()),
            CheckCode::F831 => CheckKind::DuplicateArgumentName,
            CheckCode::F841 => CheckKind::UnusedVariable("...".to_string()),
            CheckCode::F842 => CheckKind::UnusedAnnotation("...".to_string()),
            CheckCode::F901 => CheckKind::RaiseNotImplemented,
            // pylint
            CheckCode::PLC0414 => CheckKind::UselessImportAlias,
            CheckCode::PLC2201 => CheckKind::MisplacedComparisonConstant("...".to_string()),
            CheckCode::PLC3002 => CheckKind::UnnecessaryDirectLambdaCall,
            CheckCode::PLE0117 => CheckKind::NonlocalWithoutBinding("...".to_string()),
            CheckCode::PLE0118 => CheckKind::UsedPriorGlobalDeclaration("...".to_string(), 1),
            CheckCode::PLE1142 => CheckKind::AwaitOutsideAsync,
            CheckCode::PLR0402 => {
                CheckKind::ConsiderUsingFromImport("...".to_string(), "...".to_string())
            }
            CheckCode::PLR0206 => CheckKind::PropertyWithParameters,
            CheckCode::PLR1701 => {
                CheckKind::ConsiderMergingIsinstance("...".to_string(), vec!["...".to_string()])
            }
            CheckCode::PLR1722 => CheckKind::UseSysExit("exit".to_string()),
            CheckCode::PLW0120 => CheckKind::UselessElseOnLoop,
            CheckCode::PLW0602 => CheckKind::GlobalVariableNotAssigned("...".to_string()),
            // flake8-builtins
            CheckCode::A001 => CheckKind::BuiltinVariableShadowing("...".to_string()),
            CheckCode::A002 => CheckKind::BuiltinArgumentShadowing("...".to_string()),
            CheckCode::A003 => CheckKind::BuiltinAttributeShadowing("...".to_string()),
            // flake8-bugbear
            CheckCode::B002 => CheckKind::UnaryPrefixIncrement,
            CheckCode::B003 => CheckKind::AssignmentToOsEnviron,
            CheckCode::B004 => CheckKind::UnreliableCallableCheck,
            CheckCode::B005 => CheckKind::StripWithMultiCharacters,
            CheckCode::B006 => CheckKind::MutableArgumentDefault,
            CheckCode::B007 => CheckKind::UnusedLoopControlVariable("i".to_string()),
            CheckCode::B008 => CheckKind::FunctionCallArgumentDefault(None),
            CheckCode::B009 => CheckKind::GetAttrWithConstant,
            CheckCode::B010 => CheckKind::SetAttrWithConstant,
            CheckCode::B011 => CheckKind::DoNotAssertFalse,
            CheckCode::B012 => {
                CheckKind::JumpStatementInFinally("return/continue/break".to_string())
            }
            CheckCode::B013 => {
                CheckKind::RedundantTupleInExceptionHandler("ValueError".to_string())
            }
            CheckCode::B014 => CheckKind::DuplicateHandlerException(vec!["ValueError".to_string()]),
            CheckCode::B015 => CheckKind::UselessComparison,
            CheckCode::B016 => CheckKind::CannotRaiseLiteral,
            CheckCode::B017 => CheckKind::NoAssertRaisesException,
            CheckCode::B018 => CheckKind::UselessExpression,
            CheckCode::B019 => CheckKind::CachedInstanceMethod,
            CheckCode::B020 => CheckKind::LoopVariableOverridesIterator("...".to_string()),
            CheckCode::B021 => CheckKind::FStringDocstring,
            CheckCode::B022 => CheckKind::UselessContextlibSuppress,
            CheckCode::B023 => CheckKind::FunctionUsesLoopVariable("...".to_string()),
            CheckCode::B024 => CheckKind::AbstractBaseClassWithoutAbstractMethod("...".to_string()),
            CheckCode::B025 => CheckKind::DuplicateTryBlockException("Exception".to_string()),
            CheckCode::B026 => CheckKind::StarArgUnpackingAfterKeywordArg,
            CheckCode::B027 => CheckKind::EmptyMethodWithoutAbstractDecorator("...".to_string()),
            CheckCode::B904 => CheckKind::RaiseWithoutFromInsideExcept,
            CheckCode::B905 => CheckKind::ZipWithoutExplicitStrict,
            // flake8-comprehensions
            CheckCode::C400 => CheckKind::UnnecessaryGeneratorList,
            CheckCode::C401 => CheckKind::UnnecessaryGeneratorSet,
            CheckCode::C402 => CheckKind::UnnecessaryGeneratorDict,
            CheckCode::C403 => CheckKind::UnnecessaryListComprehensionSet,
            CheckCode::C404 => CheckKind::UnnecessaryListComprehensionDict,
            CheckCode::C405 => CheckKind::UnnecessaryLiteralSet("(list|tuple)".to_string()),
            CheckCode::C406 => CheckKind::UnnecessaryLiteralDict("(list|tuple)".to_string()),
            CheckCode::C408 => {
                CheckKind::UnnecessaryCollectionCall("(dict|list|tuple)".to_string())
            }
            CheckCode::C409 => {
                CheckKind::UnnecessaryLiteralWithinTupleCall("(list|tuple)".to_string())
            }
            CheckCode::C410 => {
                CheckKind::UnnecessaryLiteralWithinListCall("(list|tuple)".to_string())
            }
            CheckCode::C411 => CheckKind::UnnecessaryListCall,
            CheckCode::C413 => {
                CheckKind::UnnecessaryCallAroundSorted("(list|reversed)".to_string())
            }
            CheckCode::C414 => CheckKind::UnnecessaryDoubleCastOrProcess(
                "(list|reversed|set|sorted|tuple)".to_string(),
                "(list|set|sorted|tuple)".to_string(),
            ),
            CheckCode::C415 => {
                CheckKind::UnnecessarySubscriptReversal("(reversed|set|sorted)".to_string())
            }
            CheckCode::C416 => CheckKind::UnnecessaryComprehension("(list|set)".to_string()),
            CheckCode::C417 => CheckKind::UnnecessaryMap("(list|set|dict)".to_string()),
            // flake8-debugger
            CheckCode::T100 => CheckKind::Debugger(DebuggerUsingType::Import("...".to_string())),
            // flake8-tidy-imports
            CheckCode::TID252 => CheckKind::BannedRelativeImport(Strictness::All),
            // flake8-return
            CheckCode::RET501 => CheckKind::UnnecessaryReturnNone,
            CheckCode::RET502 => CheckKind::ImplicitReturnValue,
            CheckCode::RET503 => CheckKind::ImplicitReturn,
            CheckCode::RET504 => CheckKind::UnnecessaryAssign,
            CheckCode::RET505 => CheckKind::SuperfluousElseReturn(Branch::Else),
            CheckCode::RET506 => CheckKind::SuperfluousElseRaise(Branch::Else),
            CheckCode::RET507 => CheckKind::SuperfluousElseContinue(Branch::Else),
            CheckCode::RET508 => CheckKind::SuperfluousElseBreak(Branch::Else),
            // flake8-print
            CheckCode::T201 => CheckKind::PrintFound,
            CheckCode::T203 => CheckKind::PPrintFound,
            // flake8-quotes
            CheckCode::Q000 => CheckKind::BadQuotesInlineString(Quote::Double),
            CheckCode::Q001 => CheckKind::BadQuotesMultilineString(Quote::Double),
            CheckCode::Q002 => CheckKind::BadQuotesDocstring(Quote::Double),
            CheckCode::Q003 => CheckKind::AvoidQuoteEscape,
            // flake8-annotations
            CheckCode::ANN001 => CheckKind::MissingTypeFunctionArgument("...".to_string()),
            CheckCode::ANN002 => CheckKind::MissingTypeArgs("...".to_string()),
            CheckCode::ANN003 => CheckKind::MissingTypeKwargs("...".to_string()),
            CheckCode::ANN101 => CheckKind::MissingTypeSelf("...".to_string()),
            CheckCode::ANN102 => CheckKind::MissingTypeCls("...".to_string()),
            CheckCode::ANN201 => CheckKind::MissingReturnTypePublicFunction("...".to_string()),
            CheckCode::ANN202 => CheckKind::MissingReturnTypePrivateFunction("...".to_string()),
            CheckCode::ANN204 => CheckKind::MissingReturnTypeMagicMethod("...".to_string()),
            CheckCode::ANN205 => CheckKind::MissingReturnTypeStaticMethod("...".to_string()),
            CheckCode::ANN206 => CheckKind::MissingReturnTypeClassMethod("...".to_string()),
            CheckCode::ANN401 => CheckKind::DynamicallyTypedExpression("...".to_string()),
            // flake8-2020
            CheckCode::YTT101 => CheckKind::SysVersionSlice3Referenced,
            CheckCode::YTT102 => CheckKind::SysVersion2Referenced,
            CheckCode::YTT103 => CheckKind::SysVersionCmpStr3,
            CheckCode::YTT201 => CheckKind::SysVersionInfo0Eq3Referenced,
            CheckCode::YTT202 => CheckKind::SixPY3Referenced,
            CheckCode::YTT203 => CheckKind::SysVersionInfo1CmpInt,
            CheckCode::YTT204 => CheckKind::SysVersionInfoMinorCmpInt,
            CheckCode::YTT301 => CheckKind::SysVersion0Referenced,
            CheckCode::YTT302 => CheckKind::SysVersionCmpStr10,
            CheckCode::YTT303 => CheckKind::SysVersionSlice1Referenced,
            // flake8-blind-except
            CheckCode::BLE001 => CheckKind::BlindExcept("Exception".to_string()),
            // pyupgrade
            CheckCode::UP001 => CheckKind::UselessMetaclassType,
            CheckCode::UP003 => CheckKind::TypeOfPrimitive(Primitive::Str),
            CheckCode::UP004 => CheckKind::UselessObjectInheritance("...".to_string()),
            CheckCode::UP005 => CheckKind::DeprecatedUnittestAlias(
                "assertEquals".to_string(),
                "assertEqual".to_string(),
            ),
            CheckCode::UP006 => CheckKind::UsePEP585Annotation("List".to_string()),
            CheckCode::UP007 => CheckKind::UsePEP604Annotation,
            CheckCode::UP008 => CheckKind::SuperCallWithParameters,
            CheckCode::UP009 => CheckKind::PEP3120UnnecessaryCodingComment,
            CheckCode::UP010 => CheckKind::UnnecessaryFutureImport(vec!["...".to_string()]),
            CheckCode::UP011 => CheckKind::UnnecessaryLRUCacheParams,
            CheckCode::UP012 => CheckKind::UnnecessaryEncodeUTF8,
            CheckCode::UP013 => CheckKind::ConvertTypedDictFunctionalToClass("...".to_string()),
            CheckCode::UP014 => CheckKind::ConvertNamedTupleFunctionalToClass("...".to_string()),
            CheckCode::UP015 => CheckKind::RedundantOpenModes,
            // pydocstyle
            CheckCode::D100 => CheckKind::PublicModule,
            CheckCode::D101 => CheckKind::PublicClass,
            CheckCode::D102 => CheckKind::PublicMethod,
            CheckCode::D103 => CheckKind::PublicFunction,
            CheckCode::D104 => CheckKind::PublicPackage,
            CheckCode::D105 => CheckKind::MagicMethod,
            CheckCode::D106 => CheckKind::PublicNestedClass,
            CheckCode::D107 => CheckKind::PublicInit,
            CheckCode::D200 => CheckKind::FitsOnOneLine,
            CheckCode::D201 => CheckKind::NoBlankLineBeforeFunction(1),
            CheckCode::D202 => CheckKind::NoBlankLineAfterFunction(1),
            CheckCode::D203 => CheckKind::OneBlankLineBeforeClass(0),
            CheckCode::D204 => CheckKind::OneBlankLineAfterClass(0),
            CheckCode::D205 => CheckKind::BlankLineAfterSummary,
            CheckCode::D206 => CheckKind::IndentWithSpaces,
            CheckCode::D207 => CheckKind::NoUnderIndentation,
            CheckCode::D208 => CheckKind::NoOverIndentation,
            CheckCode::D209 => CheckKind::NewLineAfterLastParagraph,
            CheckCode::D210 => CheckKind::NoSurroundingWhitespace,
            CheckCode::D211 => CheckKind::NoBlankLineBeforeClass(1),
            CheckCode::D212 => CheckKind::MultiLineSummaryFirstLine,
            CheckCode::D213 => CheckKind::MultiLineSummarySecondLine,
            CheckCode::D214 => CheckKind::SectionNotOverIndented("Returns".to_string()),
            CheckCode::D215 => CheckKind::SectionUnderlineNotOverIndented("Returns".to_string()),
            CheckCode::D300 => CheckKind::UsesTripleQuotes,
            CheckCode::D301 => CheckKind::UsesRPrefixForBackslashedContent,
            CheckCode::D400 => CheckKind::EndsInPeriod,
            CheckCode::D402 => CheckKind::NoSignature,
            CheckCode::D403 => CheckKind::FirstLineCapitalized,
            CheckCode::D404 => CheckKind::NoThisPrefix,
            CheckCode::D405 => CheckKind::CapitalizeSectionName("returns".to_string()),
            CheckCode::D406 => CheckKind::NewLineAfterSectionName("Returns".to_string()),
            CheckCode::D407 => CheckKind::DashedUnderlineAfterSection("Returns".to_string()),
            CheckCode::D408 => CheckKind::SectionUnderlineAfterName("Returns".to_string()),
            CheckCode::D409 => {
                CheckKind::SectionUnderlineMatchesSectionLength("Returns".to_string())
            }
            CheckCode::D410 => CheckKind::BlankLineAfterSection("Returns".to_string()),
            CheckCode::D411 => CheckKind::BlankLineBeforeSection("Returns".to_string()),
            CheckCode::D412 => {
                CheckKind::NoBlankLinesBetweenHeaderAndContent("Returns".to_string())
            }
            CheckCode::D413 => CheckKind::BlankLineAfterLastSection("Returns".to_string()),
            CheckCode::D414 => CheckKind::NonEmptySection("Returns".to_string()),
            CheckCode::D415 => CheckKind::EndsInPunctuation,
            CheckCode::D416 => CheckKind::SectionNameEndsInColon("Returns".to_string()),
            CheckCode::D417 => {
                CheckKind::DocumentAllArguments(vec!["x".to_string(), "y".to_string()])
            }
            CheckCode::D418 => CheckKind::SkipDocstring,
            CheckCode::D419 => CheckKind::NonEmpty,
            // pep8-naming
            CheckCode::N801 => CheckKind::InvalidClassName("...".to_string()),
            CheckCode::N802 => CheckKind::InvalidFunctionName("...".to_string()),
            CheckCode::N803 => CheckKind::InvalidArgumentName("...".to_string()),
            CheckCode::N804 => CheckKind::InvalidFirstArgumentNameForClassMethod,
            CheckCode::N805 => CheckKind::InvalidFirstArgumentNameForMethod,
            CheckCode::N806 => CheckKind::NonLowercaseVariableInFunction("...".to_string()),
            CheckCode::N807 => CheckKind::DunderFunctionName,
            CheckCode::N811 => {
                CheckKind::ConstantImportedAsNonConstant("...".to_string(), "...".to_string())
            }
            CheckCode::N812 => {
                CheckKind::LowercaseImportedAsNonLowercase("...".to_string(), "...".to_string())
            }
            CheckCode::N813 => {
                CheckKind::CamelcaseImportedAsLowercase("...".to_string(), "...".to_string())
            }
            CheckCode::N814 => {
                CheckKind::CamelcaseImportedAsConstant("...".to_string(), "...".to_string())
            }
            CheckCode::N815 => CheckKind::MixedCaseVariableInClassScope("mixedCase".to_string()),
            CheckCode::N816 => CheckKind::MixedCaseVariableInGlobalScope("mixedCase".to_string()),
            CheckCode::N817 => {
                CheckKind::CamelcaseImportedAsAcronym("...".to_string(), "...".to_string())
            }
            CheckCode::N818 => CheckKind::ErrorSuffixOnExceptionName("...".to_string()),
            // isort
            CheckCode::I001 => CheckKind::UnsortedImports,
            // eradicate
            CheckCode::ERA001 => CheckKind::CommentedOutCode,
            // flake8-bandit
            CheckCode::S101 => CheckKind::AssertUsed,
            CheckCode::S102 => CheckKind::ExecUsed,
            CheckCode::S104 => CheckKind::HardcodedBindAllInterfaces,
            CheckCode::S105 => CheckKind::HardcodedPasswordString("...".to_string()),
            CheckCode::S106 => CheckKind::HardcodedPasswordFuncArg("...".to_string()),
            CheckCode::S107 => CheckKind::HardcodedPasswordDefault("...".to_string()),
            CheckCode::C901 => CheckKind::FunctionIsTooComplex("...".to_string(), 10),
            // flake8-boolean-trap
            CheckCode::FBT001 => CheckKind::BooleanPositionalArgInFunctionDefinition,
            CheckCode::FBT002 => CheckKind::BooleanDefaultValueInFunctionDefinition,
            CheckCode::FBT003 => CheckKind::BooleanPositionalValueInFunctionCall,
            // pygrep-hooks
            CheckCode::PGH001 => CheckKind::NoEval,
            // flake8-unused-arguments
            CheckCode::ARG001 => CheckKind::UnusedFunctionArgument("...".to_string()),
            CheckCode::ARG002 => CheckKind::UnusedMethodArgument("...".to_string()),
            CheckCode::ARG003 => CheckKind::UnusedClassMethodArgument("...".to_string()),
            CheckCode::ARG004 => CheckKind::UnusedStaticMethodArgument("...".to_string()),
            CheckCode::ARG005 => CheckKind::UnusedLambdaArgument("...".to_string()),
            // flake8-import-conventions
            CheckCode::ICN001 => {
                CheckKind::ImportAliasIsNotConventional("...".to_string(), "...".to_string())
            }
            // Ruff
            CheckCode::RUF001 => CheckKind::AmbiguousUnicodeCharacterString('𝐁', 'B'),
            CheckCode::RUF002 => CheckKind::AmbiguousUnicodeCharacterDocstring('𝐁', 'B'),
            CheckCode::RUF003 => CheckKind::AmbiguousUnicodeCharacterComment('𝐁', 'B'),
            CheckCode::RUF100 => CheckKind::UnusedNOQA(None),
        }
    }

    pub fn category(&self) -> CheckCategory {
        #[allow(clippy::match_same_arms)]
        match self {
            CheckCode::A001 => CheckCategory::Flake8Builtins,
            CheckCode::A002 => CheckCategory::Flake8Builtins,
            CheckCode::A003 => CheckCategory::Flake8Builtins,
            CheckCode::ANN001 => CheckCategory::Flake8Annotations,
            CheckCode::ANN002 => CheckCategory::Flake8Annotations,
            CheckCode::ANN003 => CheckCategory::Flake8Annotations,
            CheckCode::ANN101 => CheckCategory::Flake8Annotations,
            CheckCode::ANN102 => CheckCategory::Flake8Annotations,
            CheckCode::ANN201 => CheckCategory::Flake8Annotations,
            CheckCode::ANN202 => CheckCategory::Flake8Annotations,
            CheckCode::ANN204 => CheckCategory::Flake8Annotations,
            CheckCode::ANN205 => CheckCategory::Flake8Annotations,
            CheckCode::ANN206 => CheckCategory::Flake8Annotations,
            CheckCode::ANN401 => CheckCategory::Flake8Annotations,
            CheckCode::ARG001 => CheckCategory::Flake8UnusedArguments,
            CheckCode::ARG002 => CheckCategory::Flake8UnusedArguments,
            CheckCode::ARG003 => CheckCategory::Flake8UnusedArguments,
            CheckCode::ARG004 => CheckCategory::Flake8UnusedArguments,
            CheckCode::ARG005 => CheckCategory::Flake8UnusedArguments,
            CheckCode::B002 => CheckCategory::Flake8Bugbear,
            CheckCode::B003 => CheckCategory::Flake8Bugbear,
            CheckCode::B004 => CheckCategory::Flake8Bugbear,
            CheckCode::B005 => CheckCategory::Flake8Bugbear,
            CheckCode::B006 => CheckCategory::Flake8Bugbear,
            CheckCode::B007 => CheckCategory::Flake8Bugbear,
            CheckCode::B008 => CheckCategory::Flake8Bugbear,
            CheckCode::B009 => CheckCategory::Flake8Bugbear,
            CheckCode::B010 => CheckCategory::Flake8Bugbear,
            CheckCode::B011 => CheckCategory::Flake8Bugbear,
            CheckCode::B012 => CheckCategory::Flake8Bugbear,
            CheckCode::B013 => CheckCategory::Flake8Bugbear,
            CheckCode::B014 => CheckCategory::Flake8Bugbear,
            CheckCode::B015 => CheckCategory::Flake8Bugbear,
            CheckCode::B016 => CheckCategory::Flake8Bugbear,
            CheckCode::B017 => CheckCategory::Flake8Bugbear,
            CheckCode::B018 => CheckCategory::Flake8Bugbear,
            CheckCode::B019 => CheckCategory::Flake8Bugbear,
            CheckCode::B020 => CheckCategory::Flake8Bugbear,
            CheckCode::B021 => CheckCategory::Flake8Bugbear,
            CheckCode::B022 => CheckCategory::Flake8Bugbear,
            CheckCode::B023 => CheckCategory::Flake8Bugbear,
            CheckCode::B024 => CheckCategory::Flake8Bugbear,
            CheckCode::B025 => CheckCategory::Flake8Bugbear,
            CheckCode::B026 => CheckCategory::Flake8Bugbear,
            CheckCode::B027 => CheckCategory::Flake8Bugbear,
            CheckCode::B904 => CheckCategory::Flake8Bugbear,
            CheckCode::B905 => CheckCategory::Flake8Bugbear,
            CheckCode::BLE001 => CheckCategory::Flake8BlindExcept,
            CheckCode::C400 => CheckCategory::Flake8Comprehensions,
            CheckCode::C401 => CheckCategory::Flake8Comprehensions,
            CheckCode::C402 => CheckCategory::Flake8Comprehensions,
            CheckCode::C403 => CheckCategory::Flake8Comprehensions,
            CheckCode::C404 => CheckCategory::Flake8Comprehensions,
            CheckCode::C405 => CheckCategory::Flake8Comprehensions,
            CheckCode::C406 => CheckCategory::Flake8Comprehensions,
            CheckCode::C408 => CheckCategory::Flake8Comprehensions,
            CheckCode::C409 => CheckCategory::Flake8Comprehensions,
            CheckCode::C410 => CheckCategory::Flake8Comprehensions,
            CheckCode::C411 => CheckCategory::Flake8Comprehensions,
            CheckCode::C413 => CheckCategory::Flake8Comprehensions,
            CheckCode::C414 => CheckCategory::Flake8Comprehensions,
            CheckCode::C415 => CheckCategory::Flake8Comprehensions,
            CheckCode::C416 => CheckCategory::Flake8Comprehensions,
            CheckCode::C417 => CheckCategory::Flake8Comprehensions,
            CheckCode::C901 => CheckCategory::McCabe,
            CheckCode::D100 => CheckCategory::Pydocstyle,
            CheckCode::D101 => CheckCategory::Pydocstyle,
            CheckCode::D102 => CheckCategory::Pydocstyle,
            CheckCode::D103 => CheckCategory::Pydocstyle,
            CheckCode::D104 => CheckCategory::Pydocstyle,
            CheckCode::D105 => CheckCategory::Pydocstyle,
            CheckCode::D106 => CheckCategory::Pydocstyle,
            CheckCode::D107 => CheckCategory::Pydocstyle,
            CheckCode::D200 => CheckCategory::Pydocstyle,
            CheckCode::D201 => CheckCategory::Pydocstyle,
            CheckCode::D202 => CheckCategory::Pydocstyle,
            CheckCode::D203 => CheckCategory::Pydocstyle,
            CheckCode::D204 => CheckCategory::Pydocstyle,
            CheckCode::D205 => CheckCategory::Pydocstyle,
            CheckCode::D206 => CheckCategory::Pydocstyle,
            CheckCode::D207 => CheckCategory::Pydocstyle,
            CheckCode::D208 => CheckCategory::Pydocstyle,
            CheckCode::D209 => CheckCategory::Pydocstyle,
            CheckCode::D210 => CheckCategory::Pydocstyle,
            CheckCode::D211 => CheckCategory::Pydocstyle,
            CheckCode::D212 => CheckCategory::Pydocstyle,
            CheckCode::D213 => CheckCategory::Pydocstyle,
            CheckCode::D214 => CheckCategory::Pydocstyle,
            CheckCode::D215 => CheckCategory::Pydocstyle,
            CheckCode::D300 => CheckCategory::Pydocstyle,
            CheckCode::D301 => CheckCategory::Pydocstyle,
            CheckCode::D400 => CheckCategory::Pydocstyle,
            CheckCode::D402 => CheckCategory::Pydocstyle,
            CheckCode::D403 => CheckCategory::Pydocstyle,
            CheckCode::D404 => CheckCategory::Pydocstyle,
            CheckCode::D405 => CheckCategory::Pydocstyle,
            CheckCode::D406 => CheckCategory::Pydocstyle,
            CheckCode::D407 => CheckCategory::Pydocstyle,
            CheckCode::D408 => CheckCategory::Pydocstyle,
            CheckCode::D409 => CheckCategory::Pydocstyle,
            CheckCode::D410 => CheckCategory::Pydocstyle,
            CheckCode::D411 => CheckCategory::Pydocstyle,
            CheckCode::D412 => CheckCategory::Pydocstyle,
            CheckCode::D413 => CheckCategory::Pydocstyle,
            CheckCode::D414 => CheckCategory::Pydocstyle,
            CheckCode::D415 => CheckCategory::Pydocstyle,
            CheckCode::D416 => CheckCategory::Pydocstyle,
            CheckCode::D417 => CheckCategory::Pydocstyle,
            CheckCode::D418 => CheckCategory::Pydocstyle,
            CheckCode::D419 => CheckCategory::Pydocstyle,
            CheckCode::E402 => CheckCategory::Pycodestyle,
            CheckCode::E501 => CheckCategory::Pycodestyle,
            CheckCode::E711 => CheckCategory::Pycodestyle,
            CheckCode::E712 => CheckCategory::Pycodestyle,
            CheckCode::E713 => CheckCategory::Pycodestyle,
            CheckCode::E714 => CheckCategory::Pycodestyle,
            CheckCode::E721 => CheckCategory::Pycodestyle,
            CheckCode::E722 => CheckCategory::Pycodestyle,
            CheckCode::E731 => CheckCategory::Pycodestyle,
            CheckCode::E741 => CheckCategory::Pycodestyle,
            CheckCode::E742 => CheckCategory::Pycodestyle,
            CheckCode::E743 => CheckCategory::Pycodestyle,
            CheckCode::E902 => CheckCategory::Pycodestyle,
            CheckCode::E999 => CheckCategory::Pycodestyle,
            CheckCode::ERA001 => CheckCategory::Eradicate,
            CheckCode::F401 => CheckCategory::Pyflakes,
            CheckCode::F402 => CheckCategory::Pyflakes,
            CheckCode::F403 => CheckCategory::Pyflakes,
            CheckCode::F404 => CheckCategory::Pyflakes,
            CheckCode::F405 => CheckCategory::Pyflakes,
            CheckCode::F406 => CheckCategory::Pyflakes,
            CheckCode::F407 => CheckCategory::Pyflakes,
            CheckCode::F501 => CheckCategory::Pyflakes,
            CheckCode::F502 => CheckCategory::Pyflakes,
            CheckCode::F503 => CheckCategory::Pyflakes,
            CheckCode::F504 => CheckCategory::Pyflakes,
            CheckCode::F505 => CheckCategory::Pyflakes,
            CheckCode::F506 => CheckCategory::Pyflakes,
            CheckCode::F507 => CheckCategory::Pyflakes,
            CheckCode::F508 => CheckCategory::Pyflakes,
            CheckCode::F509 => CheckCategory::Pyflakes,
            CheckCode::F521 => CheckCategory::Pyflakes,
            CheckCode::F522 => CheckCategory::Pyflakes,
            CheckCode::F523 => CheckCategory::Pyflakes,
            CheckCode::F524 => CheckCategory::Pyflakes,
            CheckCode::F525 => CheckCategory::Pyflakes,
            CheckCode::F541 => CheckCategory::Pyflakes,
            CheckCode::F601 => CheckCategory::Pyflakes,
            CheckCode::F602 => CheckCategory::Pyflakes,
            CheckCode::F621 => CheckCategory::Pyflakes,
            CheckCode::F622 => CheckCategory::Pyflakes,
            CheckCode::F631 => CheckCategory::Pyflakes,
            CheckCode::F632 => CheckCategory::Pyflakes,
            CheckCode::F633 => CheckCategory::Pyflakes,
            CheckCode::F634 => CheckCategory::Pyflakes,
            CheckCode::F701 => CheckCategory::Pyflakes,
            CheckCode::F702 => CheckCategory::Pyflakes,
            CheckCode::F704 => CheckCategory::Pyflakes,
            CheckCode::F706 => CheckCategory::Pyflakes,
            CheckCode::F707 => CheckCategory::Pyflakes,
            CheckCode::F722 => CheckCategory::Pyflakes,
            CheckCode::F811 => CheckCategory::Pyflakes,
            CheckCode::F821 => CheckCategory::Pyflakes,
            CheckCode::F822 => CheckCategory::Pyflakes,
            CheckCode::F823 => CheckCategory::Pyflakes,
            CheckCode::F831 => CheckCategory::Pyflakes,
            CheckCode::F841 => CheckCategory::Pyflakes,
            CheckCode::F842 => CheckCategory::Pyflakes,
            CheckCode::F901 => CheckCategory::Pyflakes,
            CheckCode::FBT001 => CheckCategory::Flake8BooleanTrap,
            CheckCode::FBT002 => CheckCategory::Flake8BooleanTrap,
            CheckCode::FBT003 => CheckCategory::Flake8BooleanTrap,
            CheckCode::I001 => CheckCategory::Isort,
            CheckCode::TID252 => CheckCategory::Flake8TidyImports,
            CheckCode::ICN001 => CheckCategory::Flake8ImportConventions,
            CheckCode::N801 => CheckCategory::PEP8Naming,
            CheckCode::N802 => CheckCategory::PEP8Naming,
            CheckCode::N803 => CheckCategory::PEP8Naming,
            CheckCode::N804 => CheckCategory::PEP8Naming,
            CheckCode::N805 => CheckCategory::PEP8Naming,
            CheckCode::N806 => CheckCategory::PEP8Naming,
            CheckCode::N807 => CheckCategory::PEP8Naming,
            CheckCode::N811 => CheckCategory::PEP8Naming,
            CheckCode::N812 => CheckCategory::PEP8Naming,
            CheckCode::N813 => CheckCategory::PEP8Naming,
            CheckCode::N814 => CheckCategory::PEP8Naming,
            CheckCode::N815 => CheckCategory::PEP8Naming,
            CheckCode::N816 => CheckCategory::PEP8Naming,
            CheckCode::N817 => CheckCategory::PEP8Naming,
            CheckCode::N818 => CheckCategory::PEP8Naming,
            CheckCode::PGH001 => CheckCategory::PygrepHooks,
            CheckCode::PLC0414 => CheckCategory::Pylint,
            CheckCode::PLC2201 => CheckCategory::Pylint,
            CheckCode::PLC3002 => CheckCategory::Pylint,
            CheckCode::PLE0117 => CheckCategory::Pylint,
            CheckCode::PLE0118 => CheckCategory::Pylint,
            CheckCode::PLE1142 => CheckCategory::Pylint,
            CheckCode::PLR0206 => CheckCategory::Pylint,
            CheckCode::PLR0402 => CheckCategory::Pylint,
            CheckCode::PLR1701 => CheckCategory::Pylint,
            CheckCode::PLR1722 => CheckCategory::Pylint,
            CheckCode::PLW0120 => CheckCategory::Pylint,
            CheckCode::PLW0602 => CheckCategory::Pylint,
            CheckCode::Q000 => CheckCategory::Flake8Quotes,
            CheckCode::Q001 => CheckCategory::Flake8Quotes,
            CheckCode::Q002 => CheckCategory::Flake8Quotes,
            CheckCode::Q003 => CheckCategory::Flake8Quotes,
            CheckCode::RET501 => CheckCategory::Flake8Return,
            CheckCode::RET502 => CheckCategory::Flake8Return,
            CheckCode::RET503 => CheckCategory::Flake8Return,
            CheckCode::RET504 => CheckCategory::Flake8Return,
            CheckCode::RET505 => CheckCategory::Flake8Return,
            CheckCode::RET506 => CheckCategory::Flake8Return,
            CheckCode::RET507 => CheckCategory::Flake8Return,
            CheckCode::RET508 => CheckCategory::Flake8Return,
            CheckCode::RUF001 => CheckCategory::Ruff,
            CheckCode::RUF002 => CheckCategory::Ruff,
            CheckCode::RUF003 => CheckCategory::Ruff,
            CheckCode::RUF100 => CheckCategory::Ruff,
            CheckCode::S101 => CheckCategory::Flake8Bandit,
            CheckCode::S102 => CheckCategory::Flake8Bandit,
            CheckCode::S104 => CheckCategory::Flake8Bandit,
            CheckCode::S105 => CheckCategory::Flake8Bandit,
            CheckCode::S106 => CheckCategory::Flake8Bandit,
            CheckCode::S107 => CheckCategory::Flake8Bandit,
            CheckCode::T100 => CheckCategory::Flake8Debugger,
            CheckCode::T201 => CheckCategory::Flake8Print,
            CheckCode::T203 => CheckCategory::Flake8Print,
            CheckCode::UP001 => CheckCategory::Pyupgrade,
            CheckCode::UP003 => CheckCategory::Pyupgrade,
            CheckCode::UP004 => CheckCategory::Pyupgrade,
            CheckCode::UP005 => CheckCategory::Pyupgrade,
            CheckCode::UP006 => CheckCategory::Pyupgrade,
            CheckCode::UP007 => CheckCategory::Pyupgrade,
            CheckCode::UP008 => CheckCategory::Pyupgrade,
            CheckCode::UP009 => CheckCategory::Pyupgrade,
            CheckCode::UP010 => CheckCategory::Pyupgrade,
            CheckCode::UP011 => CheckCategory::Pyupgrade,
            CheckCode::UP012 => CheckCategory::Pyupgrade,
            CheckCode::UP013 => CheckCategory::Pyupgrade,
            CheckCode::UP014 => CheckCategory::Pyupgrade,
            CheckCode::UP015 => CheckCategory::Pyupgrade,
            CheckCode::W292 => CheckCategory::Pycodestyle,
            CheckCode::W605 => CheckCategory::Pycodestyle,
            CheckCode::YTT101 => CheckCategory::Flake82020,
            CheckCode::YTT102 => CheckCategory::Flake82020,
            CheckCode::YTT103 => CheckCategory::Flake82020,
            CheckCode::YTT201 => CheckCategory::Flake82020,
            CheckCode::YTT202 => CheckCategory::Flake82020,
            CheckCode::YTT203 => CheckCategory::Flake82020,
            CheckCode::YTT204 => CheckCategory::Flake82020,
            CheckCode::YTT301 => CheckCategory::Flake82020,
            CheckCode::YTT302 => CheckCategory::Flake82020,
            CheckCode::YTT303 => CheckCategory::Flake82020,
        }
    }
}

impl CheckKind {
    /// A four-letter shorthand code for the check.
    pub fn code(&self) -> &'static CheckCode {
        match self {
            // pycodestyle errors
            CheckKind::AmbiguousClassName(_) => &CheckCode::E742,
            CheckKind::AmbiguousFunctionName(_) => &CheckCode::E743,
            CheckKind::AmbiguousVariableName(_) => &CheckCode::E741,
            CheckKind::AssertTuple => &CheckCode::F631,
            CheckKind::BreakOutsideLoop => &CheckCode::F701,
            CheckKind::ContinueOutsideLoop => &CheckCode::F702,
            CheckKind::DefaultExceptNotLast => &CheckCode::F707,
            CheckKind::DoNotAssignLambda => &CheckCode::E731,
            CheckKind::DoNotUseBareExcept => &CheckCode::E722,
            CheckKind::DuplicateArgumentName => &CheckCode::F831,
            CheckKind::FStringMissingPlaceholders => &CheckCode::F541,
            CheckKind::ForwardAnnotationSyntaxError(_) => &CheckCode::F722,
            CheckKind::FutureFeatureNotDefined(_) => &CheckCode::F407,
            CheckKind::IOError(_) => &CheckCode::E902,
            CheckKind::IfTuple => &CheckCode::F634,
            CheckKind::ImportShadowedByLoopVar(..) => &CheckCode::F402,
            CheckKind::ImportStarNotPermitted(_) => &CheckCode::F406,
            CheckKind::ImportStarUsage(..) => &CheckCode::F405,
            CheckKind::ImportStarUsed(_) => &CheckCode::F403,
            CheckKind::InvalidPrintSyntax => &CheckCode::F633,
            CheckKind::IsLiteral => &CheckCode::F632,
            CheckKind::LateFutureImport => &CheckCode::F404,
            CheckKind::LineTooLong(..) => &CheckCode::E501,
            CheckKind::ModuleImportNotAtTopOfFile => &CheckCode::E402,
            CheckKind::MultiValueRepeatedKeyLiteral => &CheckCode::F601,
            CheckKind::MultiValueRepeatedKeyVariable(_) => &CheckCode::F602,
            CheckKind::NoneComparison(_) => &CheckCode::E711,
            CheckKind::NotInTest => &CheckCode::E713,
            CheckKind::NotIsTest => &CheckCode::E714,
            CheckKind::PercentFormatExpectedMapping => &CheckCode::F502,
            CheckKind::PercentFormatExpectedSequence => &CheckCode::F503,
            CheckKind::PercentFormatExtraNamedArguments(_) => &CheckCode::F504,
            CheckKind::PercentFormatInvalidFormat(_) => &CheckCode::F501,
            CheckKind::PercentFormatMissingArgument(_) => &CheckCode::F505,
            CheckKind::PercentFormatMixedPositionalAndNamed => &CheckCode::F506,
            CheckKind::PercentFormatPositionalCountMismatch(..) => &CheckCode::F507,
            CheckKind::PercentFormatStarRequiresSequence => &CheckCode::F508,
            CheckKind::PercentFormatUnsupportedFormatCharacter(_) => &CheckCode::F509,
            CheckKind::RaiseNotImplemented => &CheckCode::F901,
            CheckKind::ReturnOutsideFunction => &CheckCode::F706,
            CheckKind::StringDotFormatExtraNamedArguments(_) => &CheckCode::F522,
            CheckKind::StringDotFormatExtraPositionalArguments(_) => &CheckCode::F523,
            CheckKind::StringDotFormatInvalidFormat(_) => &CheckCode::F521,
            CheckKind::StringDotFormatMissingArguments(_) => &CheckCode::F524,
            CheckKind::StringDotFormatMixingAutomatic => &CheckCode::F525,
            CheckKind::SyntaxError(_) => &CheckCode::E999,
            CheckKind::ExpressionsInStarAssignment => &CheckCode::F621,
            CheckKind::TrueFalseComparison(..) => &CheckCode::E712,
            CheckKind::TwoStarredExpressions => &CheckCode::F622,
            CheckKind::TypeComparison => &CheckCode::E721,
            CheckKind::UndefinedExport(_) => &CheckCode::F822,
            CheckKind::UndefinedLocal(_) => &CheckCode::F823,
            CheckKind::RedefinedWhileUnused(..) => &CheckCode::F811,
            CheckKind::UndefinedName(_) => &CheckCode::F821,
            CheckKind::UnusedImport(..) => &CheckCode::F401,
            CheckKind::UnusedVariable(_) => &CheckCode::F841,
            CheckKind::UnusedAnnotation(_) => &CheckCode::F842,
            CheckKind::YieldOutsideFunction(_) => &CheckCode::F704,
            // pycodestyle warnings
            CheckKind::NoNewLineAtEndOfFile => &CheckCode::W292,
            CheckKind::InvalidEscapeSequence(_) => &CheckCode::W605,
            // pylint
            CheckKind::AwaitOutsideAsync => &CheckCode::PLE1142,
            CheckKind::ConsiderMergingIsinstance(..) => &CheckCode::PLR1701,
            CheckKind::ConsiderUsingFromImport(..) => &CheckCode::PLR0402,
            CheckKind::GlobalVariableNotAssigned(..) => &CheckCode::PLW0602,
            CheckKind::MisplacedComparisonConstant(..) => &CheckCode::PLC2201,
            CheckKind::PropertyWithParameters => &CheckCode::PLR0206,
            CheckKind::UnnecessaryDirectLambdaCall => &CheckCode::PLC3002,
            CheckKind::UseSysExit(_) => &CheckCode::PLR1722,
            CheckKind::NonlocalWithoutBinding(..) => &CheckCode::PLE0117,
            CheckKind::UsedPriorGlobalDeclaration(..) => &CheckCode::PLE0118,
            CheckKind::UselessElseOnLoop => &CheckCode::PLW0120,
            CheckKind::UselessImportAlias => &CheckCode::PLC0414,
            // flake8-builtins
            CheckKind::BuiltinVariableShadowing(_) => &CheckCode::A001,
            CheckKind::BuiltinArgumentShadowing(_) => &CheckCode::A002,
            CheckKind::BuiltinAttributeShadowing(_) => &CheckCode::A003,
            // flake8-bugbear
            CheckKind::AbstractBaseClassWithoutAbstractMethod(_) => &CheckCode::B024,
            CheckKind::AssignmentToOsEnviron => &CheckCode::B003,
            CheckKind::CachedInstanceMethod => &CheckCode::B019,
            CheckKind::CannotRaiseLiteral => &CheckCode::B016,
            CheckKind::DoNotAssertFalse => &CheckCode::B011,
            CheckKind::DuplicateHandlerException(_) => &CheckCode::B014,
            CheckKind::DuplicateTryBlockException(_) => &CheckCode::B025,
            CheckKind::EmptyMethodWithoutAbstractDecorator(_) => &CheckCode::B027,
            CheckKind::FStringDocstring => &CheckCode::B021,
            CheckKind::FunctionCallArgumentDefault(_) => &CheckCode::B008,
            CheckKind::FunctionUsesLoopVariable(_) => &CheckCode::B023,
            CheckKind::GetAttrWithConstant => &CheckCode::B009,
            CheckKind::JumpStatementInFinally(_) => &CheckCode::B012,
            CheckKind::LoopVariableOverridesIterator(_) => &CheckCode::B020,
            CheckKind::MutableArgumentDefault => &CheckCode::B006,
            CheckKind::NoAssertRaisesException => &CheckCode::B017,
            CheckKind::RaiseWithoutFromInsideExcept => &CheckCode::B904,
            CheckKind::ZipWithoutExplicitStrict => &CheckCode::B905,
            CheckKind::RedundantTupleInExceptionHandler(_) => &CheckCode::B013,
            CheckKind::SetAttrWithConstant => &CheckCode::B010,
            CheckKind::StarArgUnpackingAfterKeywordArg => &CheckCode::B026,
            CheckKind::StripWithMultiCharacters => &CheckCode::B005,
            CheckKind::UnaryPrefixIncrement => &CheckCode::B002,
            CheckKind::UnreliableCallableCheck => &CheckCode::B004,
            CheckKind::UnusedLoopControlVariable(_) => &CheckCode::B007,
            CheckKind::UselessComparison => &CheckCode::B015,
            CheckKind::UselessContextlibSuppress => &CheckCode::B022,
            CheckKind::UselessExpression => &CheckCode::B018,
            // flake8-blind-except
            CheckKind::BlindExcept(_) => &CheckCode::BLE001,
            // flake8-comprehensions
            CheckKind::UnnecessaryGeneratorList => &CheckCode::C400,
            CheckKind::UnnecessaryGeneratorSet => &CheckCode::C401,
            CheckKind::UnnecessaryGeneratorDict => &CheckCode::C402,
            CheckKind::UnnecessaryListComprehensionSet => &CheckCode::C403,
            CheckKind::UnnecessaryListComprehensionDict => &CheckCode::C404,
            CheckKind::UnnecessaryLiteralSet(_) => &CheckCode::C405,
            CheckKind::UnnecessaryLiteralDict(_) => &CheckCode::C406,
            CheckKind::UnnecessaryCollectionCall(_) => &CheckCode::C408,
            CheckKind::UnnecessaryLiteralWithinTupleCall(..) => &CheckCode::C409,
            CheckKind::UnnecessaryLiteralWithinListCall(..) => &CheckCode::C410,
            CheckKind::UnnecessaryListCall => &CheckCode::C411,
            CheckKind::UnnecessaryCallAroundSorted(_) => &CheckCode::C413,
            CheckKind::UnnecessaryDoubleCastOrProcess(..) => &CheckCode::C414,
            CheckKind::UnnecessarySubscriptReversal(_) => &CheckCode::C415,
            CheckKind::UnnecessaryComprehension(..) => &CheckCode::C416,
            CheckKind::UnnecessaryMap(_) => &CheckCode::C417,
            // flake8-debugger
            CheckKind::Debugger(_) => &CheckCode::T100,
            // flake8-tidy-imports
            CheckKind::BannedRelativeImport(_) => &CheckCode::TID252,
            // flake8-return
            CheckKind::UnnecessaryReturnNone => &CheckCode::RET501,
            CheckKind::ImplicitReturnValue => &CheckCode::RET502,
            CheckKind::ImplicitReturn => &CheckCode::RET503,
            CheckKind::UnnecessaryAssign => &CheckCode::RET504,
            CheckKind::SuperfluousElseReturn(_) => &CheckCode::RET505,
            CheckKind::SuperfluousElseRaise(_) => &CheckCode::RET506,
            CheckKind::SuperfluousElseContinue(_) => &CheckCode::RET507,
            CheckKind::SuperfluousElseBreak(_) => &CheckCode::RET508,
            // flake8-print
            CheckKind::PrintFound => &CheckCode::T201,
            CheckKind::PPrintFound => &CheckCode::T203,
            // flake8-quotes
            CheckKind::BadQuotesInlineString(_) => &CheckCode::Q000,
            CheckKind::BadQuotesMultilineString(_) => &CheckCode::Q001,
            CheckKind::BadQuotesDocstring(_) => &CheckCode::Q002,
            CheckKind::AvoidQuoteEscape => &CheckCode::Q003,
            // flake8-annotations
            CheckKind::MissingTypeFunctionArgument(_) => &CheckCode::ANN001,
            CheckKind::MissingTypeArgs(_) => &CheckCode::ANN002,
            CheckKind::MissingTypeKwargs(_) => &CheckCode::ANN003,
            CheckKind::MissingTypeSelf(_) => &CheckCode::ANN101,
            CheckKind::MissingTypeCls(_) => &CheckCode::ANN102,
            CheckKind::MissingReturnTypePublicFunction(_) => &CheckCode::ANN201,
            CheckKind::MissingReturnTypePrivateFunction(_) => &CheckCode::ANN202,
            CheckKind::MissingReturnTypeMagicMethod(_) => &CheckCode::ANN204,
            CheckKind::MissingReturnTypeStaticMethod(_) => &CheckCode::ANN205,
            CheckKind::MissingReturnTypeClassMethod(_) => &CheckCode::ANN206,
            CheckKind::DynamicallyTypedExpression(_) => &CheckCode::ANN401,
            // flake8-2020
            CheckKind::SysVersionSlice3Referenced => &CheckCode::YTT101,
            CheckKind::SysVersion2Referenced => &CheckCode::YTT102,
            CheckKind::SysVersionCmpStr3 => &CheckCode::YTT103,
            CheckKind::SysVersionInfo0Eq3Referenced => &CheckCode::YTT201,
            CheckKind::SixPY3Referenced => &CheckCode::YTT202,
            CheckKind::SysVersionInfo1CmpInt => &CheckCode::YTT203,
            CheckKind::SysVersionInfoMinorCmpInt => &CheckCode::YTT204,
            CheckKind::SysVersion0Referenced => &CheckCode::YTT301,
            CheckKind::SysVersionCmpStr10 => &CheckCode::YTT302,
            CheckKind::SysVersionSlice1Referenced => &CheckCode::YTT303,
            // pyupgrade
            CheckKind::TypeOfPrimitive(_) => &CheckCode::UP003,
            CheckKind::UselessMetaclassType => &CheckCode::UP001,
            CheckKind::DeprecatedUnittestAlias(..) => &CheckCode::UP005,
            CheckKind::UsePEP585Annotation(_) => &CheckCode::UP006,
            CheckKind::UsePEP604Annotation => &CheckCode::UP007,
            CheckKind::UselessObjectInheritance(_) => &CheckCode::UP004,
            CheckKind::SuperCallWithParameters => &CheckCode::UP008,
            CheckKind::PEP3120UnnecessaryCodingComment => &CheckCode::UP009,
            CheckKind::UnnecessaryFutureImport(_) => &CheckCode::UP010,
            CheckKind::UnnecessaryLRUCacheParams => &CheckCode::UP011,
            CheckKind::UnnecessaryEncodeUTF8 => &CheckCode::UP012,
            CheckKind::ConvertTypedDictFunctionalToClass(_) => &CheckCode::UP013,
            CheckKind::ConvertNamedTupleFunctionalToClass(_) => &CheckCode::UP014,
            CheckKind::RedundantOpenModes => &CheckCode::UP015,
            // pydocstyle
            CheckKind::BlankLineAfterLastSection(_) => &CheckCode::D413,
            CheckKind::BlankLineAfterSection(_) => &CheckCode::D410,
            CheckKind::BlankLineBeforeSection(_) => &CheckCode::D411,
            CheckKind::CapitalizeSectionName(_) => &CheckCode::D405,
            CheckKind::DashedUnderlineAfterSection(_) => &CheckCode::D407,
            CheckKind::DocumentAllArguments(_) => &CheckCode::D417,
            CheckKind::EndsInPeriod => &CheckCode::D400,
            CheckKind::EndsInPunctuation => &CheckCode::D415,
            CheckKind::FirstLineCapitalized => &CheckCode::D403,
            CheckKind::FitsOnOneLine => &CheckCode::D200,
            CheckKind::IndentWithSpaces => &CheckCode::D206,
            CheckKind::MagicMethod => &CheckCode::D105,
            CheckKind::MultiLineSummaryFirstLine => &CheckCode::D212,
            CheckKind::MultiLineSummarySecondLine => &CheckCode::D213,
            CheckKind::NewLineAfterLastParagraph => &CheckCode::D209,
            CheckKind::NewLineAfterSectionName(_) => &CheckCode::D406,
            CheckKind::NoBlankLineAfterFunction(_) => &CheckCode::D202,
            CheckKind::BlankLineAfterSummary => &CheckCode::D205,
            CheckKind::NoBlankLineBeforeClass(_) => &CheckCode::D211,
            CheckKind::NoBlankLineBeforeFunction(_) => &CheckCode::D201,
            CheckKind::NoBlankLinesBetweenHeaderAndContent(_) => &CheckCode::D412,
            CheckKind::NoOverIndentation => &CheckCode::D208,
            CheckKind::NoSignature => &CheckCode::D402,
            CheckKind::NoSurroundingWhitespace => &CheckCode::D210,
            CheckKind::NoThisPrefix => &CheckCode::D404,
            CheckKind::NoUnderIndentation => &CheckCode::D207,
            CheckKind::NonEmpty => &CheckCode::D419,
            CheckKind::NonEmptySection(_) => &CheckCode::D414,
            CheckKind::OneBlankLineAfterClass(_) => &CheckCode::D204,
            CheckKind::OneBlankLineBeforeClass(_) => &CheckCode::D203,
            CheckKind::PublicClass => &CheckCode::D101,
            CheckKind::PublicFunction => &CheckCode::D103,
            CheckKind::PublicInit => &CheckCode::D107,
            CheckKind::PublicMethod => &CheckCode::D102,
            CheckKind::PublicModule => &CheckCode::D100,
            CheckKind::PublicNestedClass => &CheckCode::D106,
            CheckKind::PublicPackage => &CheckCode::D104,
            CheckKind::SectionNameEndsInColon(_) => &CheckCode::D416,
            CheckKind::SectionNotOverIndented(_) => &CheckCode::D214,
            CheckKind::SectionUnderlineAfterName(_) => &CheckCode::D408,
            CheckKind::SectionUnderlineMatchesSectionLength(_) => &CheckCode::D409,
            CheckKind::SectionUnderlineNotOverIndented(_) => &CheckCode::D215,
            CheckKind::SkipDocstring => &CheckCode::D418,
            CheckKind::UsesRPrefixForBackslashedContent => &CheckCode::D301,
            CheckKind::UsesTripleQuotes => &CheckCode::D300,
            // pep8-naming
            CheckKind::InvalidClassName(_) => &CheckCode::N801,
            CheckKind::InvalidFunctionName(_) => &CheckCode::N802,
            CheckKind::InvalidArgumentName(_) => &CheckCode::N803,
            CheckKind::InvalidFirstArgumentNameForClassMethod => &CheckCode::N804,
            CheckKind::InvalidFirstArgumentNameForMethod => &CheckCode::N805,
            CheckKind::NonLowercaseVariableInFunction(..) => &CheckCode::N806,
            CheckKind::DunderFunctionName => &CheckCode::N807,
            CheckKind::ConstantImportedAsNonConstant(..) => &CheckCode::N811,
            CheckKind::LowercaseImportedAsNonLowercase(..) => &CheckCode::N812,
            CheckKind::CamelcaseImportedAsLowercase(..) => &CheckCode::N813,
            CheckKind::CamelcaseImportedAsConstant(..) => &CheckCode::N814,
            CheckKind::MixedCaseVariableInClassScope(..) => &CheckCode::N815,
            CheckKind::MixedCaseVariableInGlobalScope(..) => &CheckCode::N816,
            CheckKind::CamelcaseImportedAsAcronym(..) => &CheckCode::N817,
            CheckKind::ErrorSuffixOnExceptionName(..) => &CheckCode::N818,
            // isort
            CheckKind::UnsortedImports => &CheckCode::I001,
            // eradicate
            CheckKind::CommentedOutCode => &CheckCode::ERA001,
            // flake8-bandit
            CheckKind::AssertUsed => &CheckCode::S101,
            CheckKind::ExecUsed => &CheckCode::S102,
            CheckKind::HardcodedBindAllInterfaces => &CheckCode::S104,
            CheckKind::HardcodedPasswordString(..) => &CheckCode::S105,
            CheckKind::HardcodedPasswordFuncArg(..) => &CheckCode::S106,
            CheckKind::HardcodedPasswordDefault(..) => &CheckCode::S107,
            // mccabe
            CheckKind::FunctionIsTooComplex(..) => &CheckCode::C901,
            // flake8-boolean-trap
            CheckKind::BooleanPositionalArgInFunctionDefinition => &CheckCode::FBT001,
            CheckKind::BooleanDefaultValueInFunctionDefinition => &CheckCode::FBT002,
            CheckKind::BooleanPositionalValueInFunctionCall => &CheckCode::FBT003,
            // pygrep-hooks
            CheckKind::NoEval => &CheckCode::PGH001,
            // flake8-unused-arguments
            CheckKind::UnusedFunctionArgument(..) => &CheckCode::ARG001,
            CheckKind::UnusedMethodArgument(..) => &CheckCode::ARG002,
            CheckKind::UnusedClassMethodArgument(..) => &CheckCode::ARG003,
            CheckKind::UnusedStaticMethodArgument(..) => &CheckCode::ARG004,
            CheckKind::UnusedLambdaArgument(..) => &CheckCode::ARG005,
            // flake8-import-conventions
            CheckKind::ImportAliasIsNotConventional(..) => &CheckCode::ICN001,
            // Ruff
            CheckKind::AmbiguousUnicodeCharacterString(..) => &CheckCode::RUF001,
            CheckKind::AmbiguousUnicodeCharacterDocstring(..) => &CheckCode::RUF002,
            CheckKind::AmbiguousUnicodeCharacterComment(..) => &CheckCode::RUF003,
            CheckKind::UnusedNOQA(_) => &CheckCode::RUF100,
        }
    }

    /// The body text for the check.
    pub fn body(&self) -> String {
        match self {
            // pycodestyle errors
            CheckKind::AmbiguousClassName(name) => {
                format!("Ambiguous class name: `{name}`")
            }
            CheckKind::AmbiguousFunctionName(name) => {
                format!("Ambiguous function name: `{name}`")
            }
            CheckKind::AmbiguousVariableName(name) => {
                format!("Ambiguous variable name: `{name}`")
            }
            CheckKind::AssertTuple => {
                "Assert test is a non-empty tuple, which is always `True`".to_string()
            }
            CheckKind::BreakOutsideLoop => "`break` outside loop".to_string(),
            CheckKind::ContinueOutsideLoop => "`continue` not properly in loop".to_string(),
            CheckKind::DefaultExceptNotLast => {
                "An `except` block as not the last exception handler".to_string()
            }
            CheckKind::DoNotAssignLambda => {
                "Do not assign a lambda expression, use a def".to_string()
            }
            CheckKind::DoNotUseBareExcept => "Do not use bare `except`".to_string(),
            CheckKind::DuplicateArgumentName => {
                "Duplicate argument name in function definition".to_string()
            }
            CheckKind::ForwardAnnotationSyntaxError(body) => {
                format!("Syntax error in forward annotation: `{body}`")
            }
            CheckKind::FStringMissingPlaceholders => {
                "f-string without any placeholders".to_string()
            }
            CheckKind::FutureFeatureNotDefined(name) => {
                format!("Future feature `{name}` is not defined")
            }
            CheckKind::IOError(message) => message.clone(),
            CheckKind::IfTuple => "If test is a tuple, which is always `True`".to_string(),
            CheckKind::InvalidPrintSyntax => {
                "Use of `>>` is invalid with `print` function".to_string()
            }
            CheckKind::ImportShadowedByLoopVar(name, line) => {
                format!("Import `{name}` from line {line} shadowed by loop variable")
            }
            CheckKind::ImportStarNotPermitted(name) => {
                format!("`from {name} import *` only allowed at module level")
            }
            CheckKind::ImportStarUsed(name) => {
                format!("`from {name} import *` used; unable to detect undefined names")
            }
            CheckKind::ImportStarUsage(name, sources) => {
                let sources = sources
                    .iter()
                    .map(|source| format!("`{source}`"))
                    .join(", ");
                format!("`{name}` may be undefined, or defined from star imports: {sources}")
            }
            CheckKind::IsLiteral => "Use `==` and `!=` to compare constant literals".to_string(),
            CheckKind::LateFutureImport => {
                "`from __future__` imports must occur at the beginning of the file".to_string()
            }
            CheckKind::LineTooLong(length, limit) => {
                format!("Line too long ({length} > {limit} characters)")
            }
            CheckKind::ModuleImportNotAtTopOfFile => {
                "Module level import not at top of file".to_string()
            }
            CheckKind::MultiValueRepeatedKeyLiteral => {
                "Dictionary key literal repeated".to_string()
            }
            CheckKind::MultiValueRepeatedKeyVariable(name) => {
                format!("Dictionary key `{name}` repeated")
            }
            CheckKind::NoneComparison(op) => match op {
                RejectedCmpop::Eq => "Comparison to `None` should be `cond is None`".to_string(),
                RejectedCmpop::NotEq => {
                    "Comparison to `None` should be `cond is not None`".to_string()
                }
            },
            CheckKind::NotInTest => "Test for membership should be `not in`".to_string(),
            CheckKind::NotIsTest => "Test for object identity should be `is not`".to_string(),
            CheckKind::PercentFormatInvalidFormat(message) => {
                format!("'...' % ... has invalid format string: {message}")
            }
            CheckKind::PercentFormatUnsupportedFormatCharacter(char) => {
                format!("'...' % ... has unsupported format character '{char}'")
            }
            CheckKind::PercentFormatExpectedMapping => {
                "'...' % ... expected mapping but got sequence".to_string()
            }
            CheckKind::PercentFormatExpectedSequence => {
                "'...' % ... expected sequence but got mapping".to_string()
            }
            CheckKind::PercentFormatExtraNamedArguments(missing) => {
                let message = missing.join(", ");
                format!("'...' % ... has unused named argument(s): {message}")
            }
            CheckKind::PercentFormatMissingArgument(missing) => {
                let message = missing.join(", ");
                format!("'...' % ... is missing argument(s) for placeholder(s): {message}")
            }
            CheckKind::PercentFormatMixedPositionalAndNamed => {
                "'...' % ... has mixed positional and named placeholders".to_string()
            }
            CheckKind::PercentFormatPositionalCountMismatch(wanted, got) => {
                format!("'...' % ... has {wanted} placeholder(s) but {got} substitution(s)")
            }
            CheckKind::PercentFormatStarRequiresSequence => {
                "'...' % ... `*` specifier requires sequence".to_string()
            }
            CheckKind::RaiseNotImplemented => {
                "`raise NotImplemented` should be `raise NotImplementedError`".to_string()
            }
            CheckKind::RedefinedWhileUnused(name, line) => {
                format!("Redefinition of unused `{name}` from line {line}")
            }
            CheckKind::ReturnOutsideFunction => {
                "`return` statement outside of a function/method".to_string()
            }
            CheckKind::StringDotFormatExtraNamedArguments(missing) => {
                let message = missing.join(", ");
                format!("'...'.format(...) has unused named argument(s): {message}")
            }
            CheckKind::StringDotFormatExtraPositionalArguments(missing) => {
                let message = missing.join(", ");
                format!("'...'.format(...) has unused arguments at position(s): {message}")
            }
            CheckKind::StringDotFormatInvalidFormat(message) => {
                format!("'...'.format(...) has invalid format string: {message}")
            }
            CheckKind::StringDotFormatMissingArguments(missing) => {
                let message = missing.join(", ");
                format!("'...'.format(...) is missing argument(s) for placeholder(s): {message}")
            }
            CheckKind::StringDotFormatMixingAutomatic => {
                "'...'.format(...) mixes automatic and manual numbering".to_string()
            }
            CheckKind::SyntaxError(message) => format!("SyntaxError: {message}"),
            CheckKind::ExpressionsInStarAssignment => {
                "Too many expressions in star-unpacking assignment".to_string()
            }
            CheckKind::TrueFalseComparison(true, RejectedCmpop::Eq) => {
                "Comparison to `True` should be `cond is True`".to_string()
            }
            CheckKind::TrueFalseComparison(true, RejectedCmpop::NotEq) => {
                "Comparison to `True` should be `cond is not True`".to_string()
            }
            CheckKind::TrueFalseComparison(false, RejectedCmpop::Eq) => {
                "Comparison to `False` should be `cond is False`".to_string()
            }
            CheckKind::TrueFalseComparison(false, RejectedCmpop::NotEq) => {
                "Comparison to `False` should be `cond is not False`".to_string()
            }
            CheckKind::TwoStarredExpressions => "Two starred expressions in assignment".to_string(),
            CheckKind::TypeComparison => "Do not compare types, use `isinstance()`".to_string(),
            CheckKind::UndefinedExport(name) => {
                format!("Undefined name `{name}` in `__all__`")
            }
            CheckKind::UndefinedLocal(name) => {
                format!("Local variable `{name}` referenced before assignment")
            }
            CheckKind::UndefinedName(name) => {
                format!("Undefined name `{name}`")
            }
            CheckKind::UnusedAnnotation(name) => {
                format!("Local variable `{name}` is annotated but never used")
            }
            CheckKind::UnusedImport(name, ignore_init) => {
                if *ignore_init {
                    format!(
                        "`{name}` imported but unused; consider adding to `__all__` or using a \
                         redundant alias"
                    )
                } else {
                    format!("`{name}` imported but unused")
                }
            }
            CheckKind::UnusedVariable(name) => {
                format!("Local variable `{name}` is assigned to but never used")
            }
            CheckKind::YieldOutsideFunction(keyword) => {
                format!("`{keyword}` statement outside of a function")
            }
            // pycodestyle warnings
            CheckKind::NoNewLineAtEndOfFile => "No newline at end of file".to_string(),
            CheckKind::InvalidEscapeSequence(char) => {
                format!("Invalid escape sequence: '\\{char}'")
            }
            // pylint
            CheckKind::UselessImportAlias => {
                "Import alias does not rename original package".to_string()
            }
            CheckKind::ConsiderMergingIsinstance(obj, types) => {
                let types = types.join(", ");
                format!("Merge these isinstance calls: `isinstance({obj}, ({types}))`")
            }
            CheckKind::MisplacedComparisonConstant(comparison) => {
                format!("Comparison should be {comparison}")
            }
            CheckKind::NonlocalWithoutBinding(name) => {
                format!("Nonlocal name `{name}` found without binding")
            }
            CheckKind::UnnecessaryDirectLambdaCall => "Lambda expression called directly. Execute \
                                                       the expression inline instead."
                .to_string(),
            CheckKind::PropertyWithParameters => {
                "Cannot have defined parameters for properties".to_string()
            }
            CheckKind::ConsiderUsingFromImport(module, name) => {
                format!("Use `from {module} import {name}` in lieu of alias")
            }
            CheckKind::UsedPriorGlobalDeclaration(name, line) => {
                format!("Name `{name}` is used prior to global declaration on line {line}")
            }
            CheckKind::GlobalVariableNotAssigned(name) => {
                format!("Using global for `{name}` but no assignment is done")
            }
            CheckKind::AwaitOutsideAsync => {
                "`await` should be used within an async function".to_string()
            }
            CheckKind::UselessElseOnLoop => "Else clause on loop without a break statement, \
                                             remove the else and de-indent all the code inside it"
                .to_string(),
            CheckKind::UseSysExit(name) => format!("Use `sys.exit()` instead of `{name}`"),
            // flake8-builtins
            CheckKind::BuiltinVariableShadowing(name) => {
                format!("Variable `{name}` is shadowing a python builtin")
            }
            CheckKind::BuiltinArgumentShadowing(name) => {
                format!("Argument `{name}` is shadowing a python builtin")
            }
            CheckKind::BuiltinAttributeShadowing(name) => {
                format!("Class attribute `{name}` is shadowing a python builtin")
            }
            // flake8-bugbear
            CheckKind::UnaryPrefixIncrement => "Python does not support the unary prefix \
                                                increment. Writing `++n` is equivalent to \
                                                `+(+(n))`, which equals `n`. You meant `n += 1`."
                .to_string(),
            CheckKind::AssignmentToOsEnviron => {
                "Assigning to `os.environ` doesn't clear the environment".to_string()
            }
            CheckKind::UnreliableCallableCheck => " Using `hasattr(x, '__call__')` to test if x \
                                                   is callable is unreliable. Use `callable(x)` \
                                                   for consistent results."
                .to_string(),
            CheckKind::StripWithMultiCharacters => {
                "Using `.strip()` with multi-character strings is misleading the reader".to_string()
            }
            CheckKind::MutableArgumentDefault => {
                "Do not use mutable data structures for argument defaults".to_string()
            }
            CheckKind::UnusedLoopControlVariable(name) => format!(
                "Loop control variable `{name}` not used within the loop body. If this is \
                 intended, start the name with an underscore."
            ),
            CheckKind::FunctionCallArgumentDefault(name) => {
                if let Some(name) = name {
                    format!("Do not perform function call `{name}` in argument defaults")
                } else {
                    "Do not perform function call in argument defaults".to_string()
                }
            }
            CheckKind::FunctionUsesLoopVariable(name) => {
                format!("Function definition does not bind loop variable `{name}`")
            }
            CheckKind::GetAttrWithConstant => "Do not call `getattr` with a constant attribute \
                                               value. It is not any safer than normal property \
                                               access."
                .to_string(),
            CheckKind::SetAttrWithConstant => "Do not call `setattr` with a constant attribute \
                                               value. It is not any safer than normal property \
                                               access."
                .to_string(),
            CheckKind::DoNotAssertFalse => "Do not `assert False` (`python -O` removes these \
                                            calls), raise `AssertionError()`"
                .to_string(),
            CheckKind::JumpStatementInFinally(name) => {
                format!("`{name}` inside finally blocks cause exceptions to be silenced")
            }
            CheckKind::RedundantTupleInExceptionHandler(name) => {
                format!(
                    "A length-one tuple literal is redundant. Write `except {name}` instead of \
                     `except ({name},)`."
                )
            }
            CheckKind::UselessComparison => "Pointless comparison. This comparison does nothing \
                                             but waste CPU instructions. Either prepend `assert` \
                                             or remove it."
                .to_string(),
            CheckKind::CannotRaiseLiteral => "Cannot raise a literal. Did you intend to return it \
                                              or raise an Exception?"
                .to_string(),
            CheckKind::DuplicateHandlerException(names) => {
                if names.len() == 1 {
                    let name = &names[0];
                    format!("Exception handler with duplicate exception: `{name}`")
                } else {
                    let names = names.iter().map(|name| format!("`{name}`")).join(", ");
                    format!("Exception handler with duplicate exceptions: {names}")
                }
            }
            CheckKind::NoAssertRaisesException => {
                "`assertRaises(Exception)` should be considered evil. It can lead to your test \
                 passing even if the code being tested is never executed due to a typo. Either \
                 assert for a more specific exception (builtin or custom), use \
                 `assertRaisesRegex`, or use the context manager form of `assertRaises`."
                    .to_string()
            }
            CheckKind::UselessExpression => {
                "Found useless expression. Either assign it to a variable or remove it.".to_string()
            }
            CheckKind::CachedInstanceMethod => "Use of `functools.lru_cache` or `functools.cache` \
                                                on methods can lead to memory leaks"
                .to_string(),
            CheckKind::LoopVariableOverridesIterator(name) => {
                format!("Loop control variable `{name}` overrides iterable it iterates")
            }
            CheckKind::FStringDocstring => "f-string used as docstring. This will be interpreted \
                                            by python as a joined string rather than a docstring."
                .to_string(),
            CheckKind::UselessContextlibSuppress => {
                "No arguments passed to `contextlib.suppress`. No exceptions will be suppressed \
                 and therefore this context manager is redundant"
                    .to_string()
            }
            CheckKind::AbstractBaseClassWithoutAbstractMethod(name) => {
                format!("`{name}` is an abstract base class, but it has no abstract methods")
            }
            CheckKind::DuplicateTryBlockException(name) => {
                format!("try-except block with duplicate exception `{name}`")
            }
            CheckKind::StarArgUnpackingAfterKeywordArg => {
                "Star-arg unpacking after a keyword argument is strongly discouraged. It only \
                 works when the keyword parameter is declared after all parameters supplied by the \
                 unpacked sequence, and this change of ordering can surprise and mislead readers."
                    .to_string()
            }
            CheckKind::EmptyMethodWithoutAbstractDecorator(name) => {
                format!(
                    "`{name}` is an empty method in an abstract base class, but has no abstract \
                     decorator"
                )
            }
            CheckKind::RaiseWithoutFromInsideExcept => {
                "Within an except clause, raise exceptions with raise ... from err or raise ... \
                 from None to distinguish them from errors in exception handling"
                    .to_string()
            }
            CheckKind::ZipWithoutExplicitStrict => {
                "`zip()` without an explicit `strict=` parameter".to_string()
            }
            // flake8-comprehensions
            CheckKind::UnnecessaryGeneratorList => {
                "Unnecessary generator (rewrite as a `list` comprehension)".to_string()
            }
            CheckKind::UnnecessaryGeneratorSet => {
                "Unnecessary generator (rewrite as a `set` comprehension)".to_string()
            }
            CheckKind::UnnecessaryGeneratorDict => {
                "Unnecessary generator (rewrite as a `dict` comprehension)".to_string()
            }
            CheckKind::UnnecessaryListComprehensionSet => {
                "Unnecessary `list` comprehension (rewrite as a `set` comprehension)".to_string()
            }
            CheckKind::UnnecessaryListComprehensionDict => {
                "Unnecessary `list` comprehension (rewrite as a `dict` comprehension)".to_string()
            }
            CheckKind::UnnecessaryLiteralSet(obj_type) => {
                format!("Unnecessary `{obj_type}` literal (rewrite as a `set` literal)")
            }
            CheckKind::UnnecessaryLiteralDict(obj_type) => {
                format!("Unnecessary `{obj_type}` literal (rewrite as a `dict` literal)")
            }
            CheckKind::UnnecessaryCollectionCall(obj_type) => {
                format!("Unnecessary `{obj_type}` call (rewrite as a literal)")
            }
            CheckKind::UnnecessaryLiteralWithinTupleCall(literal) => {
                if literal == "list" {
                    format!(
                        "Unnecessary `{literal}` literal passed to `tuple()` (rewrite as a \
                         `tuple` literal)"
                    )
                } else {
                    format!(
                        "Unnecessary `{literal}` literal passed to `tuple()` (remove the outer \
                         call to `tuple()`)"
                    )
                }
            }
            CheckKind::UnnecessaryLiteralWithinListCall(literal) => {
                if literal == "list" {
                    format!(
                        "Unnecessary `{literal}` literal passed to `list()` (remove the outer \
                         call to `list()`)"
                    )
                } else {
                    format!(
                        "Unnecessary `{literal}` literal passed to `list()` (rewrite as a `list` \
                         literal)"
                    )
                }
            }
            CheckKind::UnnecessaryListCall => {
                "Unnecessary `list` call (remove the outer call to `list()`)".to_string()
            }
            CheckKind::UnnecessaryCallAroundSorted(func) => {
                format!("Unnecessary `{func}` call around `sorted()`")
            }
            CheckKind::UnnecessaryDoubleCastOrProcess(inner, outer) => {
                format!("Unnecessary `{inner}` call within `{outer}()`")
            }
            CheckKind::UnnecessarySubscriptReversal(func) => {
                format!("Unnecessary subscript reversal of iterable within `{func}()`")
            }
            CheckKind::UnnecessaryComprehension(obj_type) => {
                format!("Unnecessary `{obj_type}` comprehension (rewrite using `{obj_type}()`)")
            }
            CheckKind::UnnecessaryMap(obj_type) => {
                if obj_type == "generator" {
                    "Unnecessary `map` usage (rewrite using a generator expression)".to_string()
                } else {
                    format!("Unnecessary `map` usage (rewrite using a `{obj_type}` comprehension)")
                }
            }
            // flake8-debugger
            CheckKind::Debugger(using_type) => match using_type {
                DebuggerUsingType::Call(name) => format!("Trace found: `{name}` used"),
                DebuggerUsingType::Import(name) => format!("Import for `{name}` found"),
            },
            // flake8-tidy-imports
            CheckKind::BannedRelativeImport(strictness) => match strictness {
                Strictness::Parents => {
                    "Relative imports from parent modules are banned".to_string()
                }
                Strictness::All => "Relative imports are banned".to_string(),
            },
            // flake8-return
            CheckKind::UnnecessaryReturnNone => "Do not explicitly `return None` in function if \
                                                 it is the only possible return value"
                .to_string(),
            CheckKind::ImplicitReturnValue => "Do not implicitly `return None` in function able \
                                               to return non-`None` value"
                .to_string(),
            CheckKind::ImplicitReturn => "Missing explicit `return` at the end of function able \
                                          to return non-`None` value"
                .to_string(),
            CheckKind::UnnecessaryAssign => {
                "Unnecessary variable assignment before `return` statement".to_string()
            }
            CheckKind::SuperfluousElseReturn(branch) => {
                format!("Unnecessary `{branch}` after `return` statement")
            }
            CheckKind::SuperfluousElseRaise(branch) => {
                format!("Unnecessary `{branch}` after `raise` statement")
            }
            CheckKind::SuperfluousElseContinue(branch) => {
                format!("Unnecessary `{branch}` after `continue` statement")
            }
            CheckKind::SuperfluousElseBreak(branch) => {
                format!("Unnecessary `{branch}` after `break` statement")
            }
            // flake8-print
            CheckKind::PrintFound => "`print` found".to_string(),
            CheckKind::PPrintFound => "`pprint` found".to_string(),
            // flake8-quotes
            CheckKind::BadQuotesInlineString(quote) => match quote {
                Quote::Single => "Double quotes found but single quotes preferred".to_string(),
                Quote::Double => "Single quotes found but double quotes preferred".to_string(),
            },
            CheckKind::BadQuotesMultilineString(quote) => match quote {
                Quote::Single => {
                    "Double quote multiline found but single quotes preferred".to_string()
                }
                Quote::Double => {
                    "Single quote multiline found but double quotes preferred".to_string()
                }
            },
            CheckKind::BadQuotesDocstring(quote) => match quote {
                Quote::Single => {
                    "Double quote docstring found but single quotes preferred".to_string()
                }
                Quote::Double => {
                    "Single quote docstring found but double quotes preferred".to_string()
                }
            },
            CheckKind::AvoidQuoteEscape => {
                "Change outer quotes to avoid escaping inner quotes".to_string()
            }
            // flake8-annotations
            CheckKind::MissingTypeFunctionArgument(name) => {
                format!("Missing type annotation for function argument `{name}`")
            }
            CheckKind::MissingTypeArgs(name) => format!("Missing type annotation for `*{name}`"),
            CheckKind::MissingTypeKwargs(name) => {
                format!("Missing type annotation for `**{name}`")
            }
            CheckKind::MissingTypeSelf(name) => {
                format!("Missing type annotation for `{name}` in method")
            }
            CheckKind::MissingTypeCls(name) => {
                format!("Missing type annotation for `{name}` in classmethod")
            }
            CheckKind::MissingReturnTypePublicFunction(name) => {
                format!("Missing return type annotation for public function `{name}`")
            }
            CheckKind::MissingReturnTypePrivateFunction(name) => {
                format!("Missing return type annotation for private function `{name}`")
            }
            CheckKind::MissingReturnTypeMagicMethod(name) => {
                format!("Missing return type annotation for magic method `{name}`")
            }
            CheckKind::MissingReturnTypeStaticMethod(name) => {
                format!("Missing return type annotation for staticmethod `{name}`")
            }
            CheckKind::MissingReturnTypeClassMethod(name) => {
                format!("Missing return type annotation for classmethod `{name}`")
            }
            CheckKind::DynamicallyTypedExpression(name) => {
                format!("Dynamically typed expressions (typing.Any) are disallowed in `{name}`")
            }
            // flake8-2020
            CheckKind::SysVersionSlice3Referenced => {
                "`sys.version[:3]` referenced (python3.10), use `sys.version_info`".to_string()
            }
            CheckKind::SysVersion2Referenced => {
                "`sys.version[2]` referenced (python3.10), use `sys.version_info`".to_string()
            }
            CheckKind::SysVersionCmpStr3 => {
                "`sys.version` compared to string (python3.10), use `sys.version_info`".to_string()
            }
            CheckKind::SysVersionInfo0Eq3Referenced => {
                "`sys.version_info[0] == 3` referenced (python4), use `>=`".to_string()
            }
            CheckKind::SixPY3Referenced => {
                "`six.PY3` referenced (python4), use `not six.PY2`".to_string()
            }
            CheckKind::SysVersionInfo1CmpInt => "`sys.version_info[1]` compared to integer \
                                                 (python4), compare `sys.version_info` to tuple"
                .to_string(),
            CheckKind::SysVersionInfoMinorCmpInt => "`sys.version_info.minor` compared to integer \
                                                     (python4), compare `sys.version_info` to \
                                                     tuple"
                .to_string(),
            CheckKind::SysVersion0Referenced => {
                "`sys.version[0]` referenced (python10), use `sys.version_info`".to_string()
            }
            CheckKind::SysVersionCmpStr10 => {
                "`sys.version` compared to string (python10), use `sys.version_info`".to_string()
            }
            CheckKind::SysVersionSlice1Referenced => {
                "`sys.version[:1]` referenced (python10), use `sys.version_info`".to_string()
            }
            // pyupgrade
            CheckKind::TypeOfPrimitive(primitive) => {
                format!("Use `{}` instead of `type(...)`", primitive.builtin())
            }
            CheckKind::UselessMetaclassType => "`__metaclass__ = type` is implied".to_string(),
            CheckKind::DeprecatedUnittestAlias(alias, target) => {
                format!("`{alias}` is deprecated, use `{target}` instead")
            }
            CheckKind::UselessObjectInheritance(name) => {
                format!("Class `{name}` inherits from object")
            }
            CheckKind::UsePEP585Annotation(name) => {
                format!(
                    "Use `{}` instead of `{}` for type annotations",
                    name.to_lowercase(),
                    name,
                )
            }
            CheckKind::UsePEP604Annotation => "Use `X | Y` for type annotations".to_string(),
            CheckKind::SuperCallWithParameters => {
                "Use `super()` instead of `super(__class__, self)`".to_string()
            }
            CheckKind::UnnecessaryFutureImport(names) => {
                if names.len() == 1 {
                    let import = &names[0];
                    format!("Unnecessary `__future__` import `{import}` for target Python version")
                } else {
                    let imports = names.iter().map(|name| format!("`{name}`")).join(", ");
                    format!("Unnecessary `__future__` imports {imports} for target Python version")
                }
            }
            CheckKind::UnnecessaryLRUCacheParams => {
                "Unnecessary parameters to `functools.lru_cache`".to_string()
            }
            CheckKind::UnnecessaryEncodeUTF8 => "Unnecessary call to `encode` as UTF-8".to_string(),
            CheckKind::RedundantOpenModes => "Unnecessary open mode parameters".to_string(),
            CheckKind::ConvertTypedDictFunctionalToClass(name) => {
                format!("Convert `{name}` from `TypedDict` functional to class syntax")
            }
            CheckKind::ConvertNamedTupleFunctionalToClass(name) => {
                format!("Convert `{name}` from `NamedTuple` functional to class syntax")
            }
            // pydocstyle
            CheckKind::FitsOnOneLine => "One-line docstring should fit on one line".to_string(),
            CheckKind::BlankLineAfterSummary => {
                "1 blank line required between summary line and description".to_string()
            }
            CheckKind::NewLineAfterLastParagraph => {
                "Multi-line docstring closing quotes should be on a separate line".to_string()
            }
            CheckKind::NoSurroundingWhitespace => {
                "No whitespaces allowed surrounding docstring text".to_string()
            }
            CheckKind::EndsInPeriod => "First line should end with a period".to_string(),
            CheckKind::NonEmpty => "Docstring is empty".to_string(),
            CheckKind::EndsInPunctuation => "First line should end with a period, question mark, \
                                             or exclamation point"
                .to_string(),
            CheckKind::FirstLineCapitalized => {
                "First word of the first line should be properly capitalized".to_string()
            }
            CheckKind::UsesRPrefixForBackslashedContent => {
                r#"Use r""" if any backslashes in a docstring"#.to_string()
            }
            CheckKind::UsesTripleQuotes => r#"Use """triple double quotes""""#.to_string(),
            CheckKind::MultiLineSummaryFirstLine => {
                "Multi-line docstring summary should start at the first line".to_string()
            }
            CheckKind::MultiLineSummarySecondLine => {
                "Multi-line docstring summary should start at the second line".to_string()
            }
            CheckKind::NoSignature => {
                "First line should not be the function's signature".to_string()
            }
            CheckKind::NoBlankLineBeforeFunction(num_lines) => {
                format!("No blank lines allowed before function docstring (found {num_lines})")
            }
            CheckKind::NoBlankLineAfterFunction(num_lines) => {
                format!("No blank lines allowed after function docstring (found {num_lines})")
            }
            CheckKind::NoBlankLineBeforeClass(_) => {
                "No blank lines allowed before class docstring".to_string()
            }
            CheckKind::OneBlankLineBeforeClass(_) => {
                "1 blank line required before class docstring".to_string()
            }
            CheckKind::OneBlankLineAfterClass(_) => {
                "1 blank line required after class docstring".to_string()
            }
            CheckKind::PublicModule => "Missing docstring in public module".to_string(),
            CheckKind::PublicClass => "Missing docstring in public class".to_string(),
            CheckKind::PublicMethod => "Missing docstring in public method".to_string(),
            CheckKind::PublicFunction => "Missing docstring in public function".to_string(),
            CheckKind::PublicPackage => "Missing docstring in public package".to_string(),
            CheckKind::MagicMethod => "Missing docstring in magic method".to_string(),
            CheckKind::PublicNestedClass => "Missing docstring in public nested class".to_string(),
            CheckKind::PublicInit => "Missing docstring in `__init__`".to_string(),
            CheckKind::NoThisPrefix => {
                "First word of the docstring should not be \"This\"".to_string()
            }
            CheckKind::SkipDocstring => {
                "Function decorated with `@overload` shouldn't contain a docstring".to_string()
            }
            CheckKind::CapitalizeSectionName(name) => {
                format!("Section name should be properly capitalized (\"{name}\")")
            }
            CheckKind::BlankLineAfterLastSection(name) => {
                format!("Missing blank line after last section (\"{name}\")")
            }
            CheckKind::BlankLineAfterSection(name) => {
                format!("Missing blank line after section (\"{name}\")")
            }
            CheckKind::BlankLineBeforeSection(name) => {
                format!("Missing blank line before section (\"{name}\")")
            }
            CheckKind::NewLineAfterSectionName(name) => {
                format!("Section name should end with a newline (\"{name}\")")
            }
            CheckKind::DashedUnderlineAfterSection(name) => {
                format!("Missing dashed underline after section (\"{name}\")")
            }
            CheckKind::SectionUnderlineAfterName(name) => {
                format!(
                    "Section underline should be in the line following the section's name \
                     (\"{name}\")"
                )
            }
            CheckKind::SectionUnderlineMatchesSectionLength(name) => {
                format!("Section underline should match the length of its name (\"{name}\")")
            }
            CheckKind::NoBlankLinesBetweenHeaderAndContent(name) => {
                format!(
                    "No blank lines allowed between a section header and its content (\"{name}\")"
                )
            }
            CheckKind::NonEmptySection(name) => format!("Section has no content (\"{name}\")"),
            CheckKind::SectionNotOverIndented(name) => {
                format!("Section is over-indented (\"{name}\")")
            }
            CheckKind::SectionUnderlineNotOverIndented(name) => {
                format!("Section underline is over-indented (\"{name}\")")
            }
            CheckKind::SectionNameEndsInColon(name) => {
                format!("Section name should end with a colon (\"{name}\")")
            }
            CheckKind::DocumentAllArguments(names) => {
                if names.len() == 1 {
                    let name = &names[0];
                    format!("Missing argument description in the docstring: `{name}`")
                } else {
                    let names = names.iter().map(|name| format!("`{name}`")).join(", ");
                    format!("Missing argument descriptions in the docstring: {names}")
                }
            }
            CheckKind::IndentWithSpaces => {
                "Docstring should be indented with spaces, not tabs".to_string()
            }
            CheckKind::NoUnderIndentation => "Docstring is under-indented".to_string(),
            CheckKind::NoOverIndentation => "Docstring is over-indented".to_string(),
            // pep8-naming
            CheckKind::InvalidClassName(name) => {
                format!("Class name `{name}` should use CapWords convention ")
            }
            CheckKind::InvalidFunctionName(name) => {
                format!("Function name `{name}` should be lowercase")
            }
            CheckKind::InvalidArgumentName(name) => {
                format!("Argument name `{name}` should be lowercase")
            }
            CheckKind::InvalidFirstArgumentNameForClassMethod => {
                "First argument of a class method should be named `cls`".to_string()
            }
            CheckKind::InvalidFirstArgumentNameForMethod => {
                "First argument of a method should be named `self`".to_string()
            }
            CheckKind::NonLowercaseVariableInFunction(name) => {
                format!("Variable `{name}` in function should be lowercase")
            }
            CheckKind::DunderFunctionName => {
                "Function name should not start and end with `__`".to_string()
            }
            CheckKind::ConstantImportedAsNonConstant(name, asname) => {
                format!("Constant `{name}` imported as non-constant `{asname}`")
            }
            CheckKind::LowercaseImportedAsNonLowercase(name, asname) => {
                format!("Lowercase `{name}` imported as non-lowercase `{asname}`")
            }
            CheckKind::CamelcaseImportedAsLowercase(name, asname) => {
                format!("Camelcase `{name}` imported as lowercase `{asname}`")
            }
            CheckKind::CamelcaseImportedAsConstant(name, asname) => {
                format!("Camelcase `{name}` imported as constant `{asname}`")
            }
            CheckKind::MixedCaseVariableInClassScope(name) => {
                format!("Variable `{name}` in class scope should not be mixedCase")
            }
            CheckKind::MixedCaseVariableInGlobalScope(name) => {
                format!("Variable `{name}` in global scope should not be mixedCase")
            }
            CheckKind::CamelcaseImportedAsAcronym(name, asname) => {
                format!("Camelcase `{name}` imported as acronym `{asname}`")
            }
            CheckKind::ErrorSuffixOnExceptionName(name) => {
                format!("Exception name `{name}` should be named with an Error suffix")
            }
            CheckKind::PEP3120UnnecessaryCodingComment => {
                "UTF-8 encoding declaration is unnecessary".to_string()
            }
            // isort
            CheckKind::UnsortedImports => "Import block is un-sorted or un-formatted".to_string(),
            // eradicate
            CheckKind::CommentedOutCode => "Found commented-out code".to_string(),
            // flake8-bandit
            CheckKind::AssertUsed => "Use of `assert` detected".to_string(),
            CheckKind::ExecUsed => "Use of `exec` detected".to_string(),
            CheckKind::HardcodedBindAllInterfaces => {
                "Possible binding to all interfaces".to_string()
            }
            CheckKind::HardcodedPasswordString(string) => {
                format!("Possible hardcoded password: `\"{string}\"`")
            }
            CheckKind::HardcodedPasswordFuncArg(string) => {
                format!("Possible hardcoded password: `\"{string}\"`")
            }
            CheckKind::HardcodedPasswordDefault(string) => {
                format!("Possible hardcoded password: `\"{string}\"`")
            }
            // flake8-blind-except
            CheckKind::BlindExcept(name) => format!("Do not catch blind exception: `{name}`"),
            // mccabe
            CheckKind::FunctionIsTooComplex(name, complexity) => {
                format!("`{name}` is too complex ({complexity})")
            }
            // flake8-boolean-trap
            CheckKind::BooleanPositionalArgInFunctionDefinition => {
                "Boolean positional arg in function definition".to_string()
            }
            CheckKind::BooleanDefaultValueInFunctionDefinition => {
                "Boolean default value in function definition".to_string()
            }
            CheckKind::BooleanPositionalValueInFunctionCall => {
                "Boolean positional value in function call".to_string()
            }
            // pygrep-hooks
            CheckKind::NoEval => "No builtin `eval()` allowed".to_string(),
            // flake8-unused-arguments
            CheckKind::UnusedFunctionArgument(name) => {
                format!("Unused function argument: `{name}`")
            }
            CheckKind::UnusedMethodArgument(name) => format!("Unused method argument: `{name}`"),
            CheckKind::UnusedClassMethodArgument(name) => {
                format!("Unused class method argument: `{name}`")
            }
            CheckKind::UnusedStaticMethodArgument(name) => {
                format!("Unused static method argument: `{name}`")
            }
            CheckKind::UnusedLambdaArgument(name) => format!("Unused lambda argument: `{name}`"),
            // flake8-import-conventions
            CheckKind::ImportAliasIsNotConventional(name, asname) => {
                format!("`{name}` should be imported as `{asname}`")
            }
            // Ruff
            CheckKind::AmbiguousUnicodeCharacterString(confusable, representant) => {
                format!(
                    "String contains ambiguous unicode character '{confusable}' (did you mean \
                     '{representant}'?)"
                )
            }
            CheckKind::AmbiguousUnicodeCharacterDocstring(confusable, representant) => {
                format!(
                    "Docstring contains ambiguous unicode character '{confusable}' (did you mean \
                     '{representant}'?)"
                )
            }
            CheckKind::AmbiguousUnicodeCharacterComment(confusable, representant) => {
                format!(
                    "Comment contains ambiguous unicode character '{confusable}' (did you mean \
                     '{representant}'?)"
                )
            }
            CheckKind::UnusedNOQA(codes) => match codes {
                None => "Unused `noqa` directive".to_string(),
                Some(codes) => {
                    let codes = codes
                        .iter()
                        .map(|code| {
                            if CheckCode::from_str(code).is_ok() {
                                code.to_string()
                            } else {
                                format!("{code} (not implemented)")
                            }
                        })
                        .join(", ");
                    format!("Unused `noqa` directive for: {codes}")
                }
            },
        }
    }

    /// The summary text for the check. Typically a truncated form of the body
    /// text.
    pub fn summary(&self) -> String {
        match self {
            CheckKind::UnaryPrefixIncrement => {
                "Python does not support the unary prefix increment".to_string()
            }
            CheckKind::UnusedLoopControlVariable(name) => {
                format!("Loop control variable `{name}` not used within the loop body")
            }
            CheckKind::NoAssertRaisesException => {
                "`assertRaises(Exception)` should be considered evil".to_string()
            }
            CheckKind::StarArgUnpackingAfterKeywordArg => {
                "Star-arg unpacking after a keyword argument is strongly discouraged".to_string()
            }
            _ => self.body(),
        }
    }

    /// Whether the check kind is (potentially) fixable.
    pub fn fixable(&self) -> bool {
        matches!(
            self,
            CheckKind::AmbiguousUnicodeCharacterString(..)
                | CheckKind::AmbiguousUnicodeCharacterDocstring(..)
                | CheckKind::BlankLineAfterLastSection(..)
                | CheckKind::BlankLineAfterSection(..)
                | CheckKind::BlankLineAfterSummary
                | CheckKind::BlankLineBeforeSection(..)
                | CheckKind::CapitalizeSectionName(..)
                | CheckKind::CommentedOutCode
                | CheckKind::ConvertNamedTupleFunctionalToClass(..)
                | CheckKind::ConvertTypedDictFunctionalToClass(..)
                | CheckKind::DashedUnderlineAfterSection(..)
                | CheckKind::DeprecatedUnittestAlias(..)
                | CheckKind::DoNotAssertFalse
                | CheckKind::DoNotAssignLambda
                | CheckKind::DuplicateHandlerException(..)
                | CheckKind::EndsInPeriod
                | CheckKind::EndsInPunctuation
                | CheckKind::GetAttrWithConstant
                | CheckKind::ImplicitReturn
                | CheckKind::ImplicitReturnValue
                | CheckKind::IsLiteral
                | CheckKind::MisplacedComparisonConstant(..)
                | CheckKind::NewLineAfterLastParagraph
                | CheckKind::NewLineAfterSectionName(..)
                | CheckKind::NoBlankLineAfterFunction(..)
                | CheckKind::NoBlankLineBeforeClass(..)
                | CheckKind::NoBlankLineBeforeFunction(..)
                | CheckKind::NoBlankLinesBetweenHeaderAndContent(..)
                | CheckKind::NoOverIndentation
                | CheckKind::NoSurroundingWhitespace
                | CheckKind::NoUnderIndentation
                | CheckKind::NoneComparison(..)
                | CheckKind::NotInTest
                | CheckKind::NotIsTest
                | CheckKind::OneBlankLineAfterClass(..)
                | CheckKind::OneBlankLineBeforeClass(..)
                | CheckKind::PEP3120UnnecessaryCodingComment
                | CheckKind::PPrintFound
                | CheckKind::PrintFound
                | CheckKind::RaiseNotImplemented
                | CheckKind::RedundantOpenModes
                | CheckKind::RedundantTupleInExceptionHandler(..)
                | CheckKind::SectionNameEndsInColon(..)
                | CheckKind::SectionNotOverIndented(..)
                | CheckKind::SectionUnderlineAfterName(..)
                | CheckKind::SectionUnderlineMatchesSectionLength(..)
                | CheckKind::SectionUnderlineNotOverIndented(..)
                | CheckKind::SetAttrWithConstant
                | CheckKind::SuperCallWithParameters
                | CheckKind::TrueFalseComparison(..)
                | CheckKind::TypeOfPrimitive(..)
                | CheckKind::UnnecessaryCollectionCall(..)
                | CheckKind::UnnecessaryComprehension(..)
                | CheckKind::UnnecessaryEncodeUTF8
                | CheckKind::UnnecessaryFutureImport(..)
                | CheckKind::UnnecessaryGeneratorDict
                | CheckKind::UnnecessaryGeneratorList
                | CheckKind::UnnecessaryGeneratorSet
                | CheckKind::UnnecessaryLRUCacheParams
                | CheckKind::UnnecessaryListCall
                | CheckKind::UnnecessaryListComprehensionDict
                | CheckKind::UnnecessaryListComprehensionSet
                | CheckKind::UnnecessaryLiteralDict(..)
                | CheckKind::UnnecessaryLiteralSet(..)
                | CheckKind::UnnecessaryLiteralWithinListCall(..)
                | CheckKind::UnnecessaryLiteralWithinTupleCall(..)
                | CheckKind::UnnecessaryReturnNone
                | CheckKind::UnsortedImports
                | CheckKind::UnusedImport(_, false)
                | CheckKind::UnusedLoopControlVariable(..)
                | CheckKind::UnusedNOQA(..)
                | CheckKind::UsePEP585Annotation(..)
                | CheckKind::UsePEP604Annotation
                | CheckKind::UseSysExit(..)
                | CheckKind::UselessImportAlias
                | CheckKind::UselessMetaclassType
                | CheckKind::UselessObjectInheritance(..)
        )
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub kind: CheckKind,
    pub location: Location,
    pub end_location: Location,
    pub fix: Option<Fix>,
}

impl Check {
    pub fn new(kind: CheckKind, range: Range) -> Self {
        Self {
            kind,
            location: range.location,
            end_location: range.end_location,
            fix: None,
        }
    }

    pub fn amend(&mut self, fix: Fix) {
        self.fix = Some(fix);
    }
}

/// A hash map from deprecated to latest `CheckCode`.
pub static CODE_REDIRECTS: Lazy<FxHashMap<&'static str, CheckCode>> = Lazy::new(|| {
    FxHashMap::from_iter([
        // TODO(charlie): Remove by 2023-01-01.
        ("U001", CheckCode::UP001),
        ("U003", CheckCode::UP003),
        ("U004", CheckCode::UP004),
        ("U005", CheckCode::UP005),
        ("U006", CheckCode::UP006),
        ("U007", CheckCode::UP007),
        ("U008", CheckCode::UP008),
        ("U009", CheckCode::UP009),
        ("U010", CheckCode::UP010),
        ("U011", CheckCode::UP011),
        ("U012", CheckCode::UP012),
        ("U013", CheckCode::UP013),
        ("U014", CheckCode::UP014),
        ("U015", CheckCode::UP015),
        // TODO(charlie): Remove by 2023-02-01.
        ("I252", CheckCode::TID252),
        ("M001", CheckCode::RUF100),
    ])
});

/// A hash map from deprecated `CheckCodePrefix` to latest `CheckCodePrefix`.
pub static PREFIX_REDIRECTS: Lazy<FxHashMap<&'static str, &'static str>> = Lazy::new(|| {
    FxHashMap::from_iter([
        // TODO(charlie): Remove by 2023-01-01.
        ("U", "UP"),
        ("U0", "UP0"),
        ("U00", "UP00"),
        ("U01", "UP01"),
        // TODO(charlie): Remove by 2023-02-01.
        ("I2", "TID2"),
        ("I25", "TID25"),
        ("M", "RUF100"),
        ("M0", "RUF100"),
    ])
});

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use strum::IntoEnumIterator;

    use crate::checks::CheckCode;

    #[test]
    fn check_code_serialization() {
        for check_code in CheckCode::iter() {
            assert!(
                CheckCode::from_str(check_code.as_ref()).is_ok(),
                "{check_code:?} could not be round-trip serialized."
            );
        }
    }
}
