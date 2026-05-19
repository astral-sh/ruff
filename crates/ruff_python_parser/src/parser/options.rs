use ruff_python_ast::{PySourceType, PythonVersion};

use crate::{AsMode, Mode};

/// The default maximum recursion depth used by the parser.
///
/// Real-world Python rarely nests more than a handful of levels deep; this cap
/// exists to keep the parser from overflowing the stack on adversarial or
/// machine-generated input.
///
/// The default value mirrors CPython's `MAXSTACK` of 200 nested parentheses
/// (`Parser/parser.c`): a one-statement module of the form `((((1))))` at
/// depth 200 must parse, and one at depth 201 must fail. Each nesting level
/// costs one `enter_recursion` call, plus two framing calls (one for the
/// surrounding statement and one for the innermost atom), so the cap is set
/// to `200 + 2`.
const DEFAULT_MAX_RECURSION_DEPTH: u16 = 202;

/// Options for controlling how a source file is parsed.
///
/// You can construct a [`ParseOptions`] directly from a [`Mode`]:
///
/// ```
/// use ruff_python_parser::{Mode, ParseOptions};
///
/// let options = ParseOptions::from(Mode::Module);
/// ```
///
/// or from a [`PySourceType`]
///
/// ```
/// use ruff_python_ast::PySourceType;
/// use ruff_python_parser::ParseOptions;
///
/// let options = ParseOptions::from(PySourceType::Python);
/// ```
#[derive(Clone, Debug)]
pub struct ParseOptions {
    /// Specify the mode in which the code will be parsed.
    pub(crate) mode: Mode,
    /// Target version for detecting version-related syntax errors.
    pub(crate) target_version: PythonVersion,
    /// Maximum recursion depth for the parser. The parser aborts with a
    /// [`crate::ParseErrorType::RecursionLimitExceeded`] error once this many
    /// nested expression / statement / pattern nodes are on the parser's call
    /// stack. Defaults to [`DEFAULT_MAX_RECURSION_DEPTH`].
    pub(crate) max_recursion_depth: u16,
}

impl ParseOptions {
    #[must_use]
    pub fn with_target_version(mut self, target_version: PythonVersion) -> Self {
        self.target_version = target_version;
        self
    }

    pub fn target_version(&self) -> PythonVersion {
        self.target_version
    }

    /// Set the maximum recursion depth for the parser.
    #[must_use]
    pub fn with_max_recursion_depth(mut self, depth: u16) -> Self {
        self.max_recursion_depth = depth;
        self
    }

    pub fn max_recursion_depth(&self) -> u16 {
        self.max_recursion_depth
    }
}

impl From<Mode> for ParseOptions {
    fn from(mode: Mode) -> Self {
        Self {
            mode,
            target_version: PythonVersion::default(),
            max_recursion_depth: DEFAULT_MAX_RECURSION_DEPTH,
        }
    }
}

impl From<PySourceType> for ParseOptions {
    fn from(source_type: PySourceType) -> Self {
        Self {
            mode: source_type.as_mode(),
            target_version: PythonVersion::default(),
            max_recursion_depth: DEFAULT_MAX_RECURSION_DEPTH,
        }
    }
}
