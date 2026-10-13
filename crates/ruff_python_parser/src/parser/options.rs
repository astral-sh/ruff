use std::num::NonZeroU32;

use ruff_python_ast::{PySourceType, PythonVersion};

use crate::{AsMode, Mode};

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
    /// Maximum number of nested recursive parser calls, if limited.
    pub(crate) max_recursion_depth: Option<NonZeroU32>,
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

    /// Limits how deeply the parser recurses before it gives up on the rest of the source.
    ///
    /// By default, the parser accepts arbitrarily deep nesting and grows its stack on the heap
    /// as needed. That is the right behavior for a linter or type checker that runs on trusted
    /// code, but it means that memory usage is bounded only by the input. Applications that parse
    /// untrusted code can set a limit to bound the stack space the parser allocates.
    ///
    /// The depth counts nested statements, expressions, patterns, and f-string format specs that
    /// each require a recursive parser call. For example, each nesting level in `((((1))))`, each
    /// nested block, and each `lambda` body adds one to the depth. It is not a count of nested
    /// parentheses: CPython's parser has its own, unrelated limits.
    ///
    /// When the limit is exceeded, the parser reports a
    /// [`ParseErrorType::RecursionLimitExceeded`](crate::ParseErrorType::RecursionLimitExceeded)
    /// error at the construct that is nested too deeply, replaces it with a placeholder node,
    /// and skips the remainder of the source.
    #[must_use]
    pub fn with_max_recursion_depth(mut self, depth: NonZeroU32) -> Self {
        self.max_recursion_depth = Some(depth);
        self
    }

    pub fn max_recursion_depth(&self) -> Option<NonZeroU32> {
        self.max_recursion_depth
    }
}

impl From<Mode> for ParseOptions {
    fn from(mode: Mode) -> Self {
        Self {
            mode,
            target_version: PythonVersion::default(),
            max_recursion_depth: None,
        }
    }
}

impl From<PySourceType> for ParseOptions {
    fn from(source_type: PySourceType) -> Self {
        Self {
            mode: source_type.as_mode(),
            target_version: PythonVersion::default(),
            max_recursion_depth: None,
        }
    }
}
