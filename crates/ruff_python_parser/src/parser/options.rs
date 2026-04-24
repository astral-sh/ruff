use ruff_python_ast::{PySourceType, PythonVersion};

use crate::{AsMode, Mode};

/// The default maximum recursion depth used by the parser.
///
/// Real-world Python rarely nests more than a handful of levels deep; this cap
/// exists to keep the parser from overflowing the stack on adversarial or
/// machine-generated input. The value is intentionally modest because each
/// "depth unit" corresponds to several real stack frames on the parser's
/// descent (for a parenthesised expression: ~8 frames, each a few KB in a
/// debug build), so one depth unit is roughly 15–30 KB of actual stack. The
/// default has to fit comfortably within the tightest stacks we care about:
/// Rust's default 2 MB worker-thread stack (used by `std::thread`, tokio,
/// `cargo test`, …) and Windows' 1 MB main-thread stack.
const DEFAULT_MAX_RECURSION_DEPTH: u16 = 200;

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
