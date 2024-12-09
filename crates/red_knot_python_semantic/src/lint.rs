use itertools::Itertools;
use ruff_db::diagnostic::{LintName, Severity};

#[derive(Debug, Clone)]
pub struct LintMetadata {
    /// The unique identifier for the lint.
    pub name: LintName,

    /// A one-sentence summary of what the lint catches.
    pub summary: &'static str,

    /// An in depth explanation of the lint in markdown. Covers what the lint does, why it's bad and possible fixes.
    ///
    /// The documentation may require post-processing to be rendered correctly. For example, lines
    /// might have leading or trailing whitespace that should be removed.
    pub raw_documentation: &'static str,

    /// The default level of the lint if the user doesn't specify one.
    pub default_level: Level,

    pub status: LintStatus,

    /// Location where this lint is declared: `file_name:line`
    pub source: &'static str,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Level {
    /// The lint is disabled and should not run.
    Ignore,

    /// The lint is enabled and diagnostic should have a warning severity.
    Warn,

    /// The lint is enabled and diagnostics have an error severity.
    Error,
}

impl Level {
    pub const fn is_error(self) -> bool {
        matches!(self, Level::Error)
    }

    pub const fn is_warn(self) -> bool {
        matches!(self, Level::Warn)
    }

    pub const fn is_ignore(self) -> bool {
        matches!(self, Level::Ignore)
    }
}

impl TryFrom<Level> for Severity {
    type Error = ();

    fn try_from(level: Level) -> Result<Self, ()> {
        match level {
            Level::Ignore => Err(()),
            Level::Warn => Ok(Severity::Warning),
            Level::Error => Ok(Severity::Error),
        }
    }
}

impl LintMetadata {
    pub fn name(&self) -> LintName {
        self.name
    }

    pub fn summary(&self) -> &str {
        self.summary
    }

    /// Returns the documentation line by line with leading and trailing whitespace removed.
    pub fn documentation_lines(&self) -> impl Iterator<Item = &str> {
        self.raw_documentation
            .lines()
            .map(|line| line.strip_prefix(' ').unwrap_or(line).trim_end())
    }

    /// Returns the documentation as a single string.
    pub fn documentation(&self) -> String {
        self.documentation_lines().join("\n")
    }

    pub fn default_level(&self) -> Level {
        self.default_level
    }

    pub fn status(&self) -> &LintStatus {
        &self.status
    }

    pub fn source(&self) -> &str {
        self.source
    }
}

#[doc(hidden)]
pub const fn lint_metadata_defaults() -> LintMetadata {
    LintMetadata {
        name: LintName::of(""),
        summary: "",
        raw_documentation: "",
        default_level: Level::Error,
        status: LintStatus::preview("0.0.0"),
        source: "",
    }
}

#[derive(Copy, Clone, Debug)]
pub enum LintStatus {
    /// The rule has been added to the linter, but is not yet stable.
    Preview {
        /// When the rule was added to preview
        since: &'static str,
    },

    /// Stable rule that was added in the version defined by `since`.
    Stable { since: &'static str },

    /// The rule has been deprecated since (version) and will be removed in the future.
    Deprecated {
        since: &'static str,
        reason: &'static str,
    },

    /// The rule has been removed since (version) and using it will result in an error.
    Removed {
        since: &'static str,
        reason: &'static str,
    },
}

impl LintStatus {
    pub const fn preview(since: &'static str) -> Self {
        LintStatus::Preview { since }
    }

    pub const fn stable(since: &'static str) -> Self {
        LintStatus::Stable { since }
    }

    pub const fn deprecated(since: &'static str, reason: &'static str) -> Self {
        LintStatus::Deprecated { since, reason }
    }

    pub const fn removed(since: &'static str, reason: &'static str) -> Self {
        LintStatus::Removed { since, reason }
    }

    pub const fn is_removed(&self) -> bool {
        matches!(self, LintStatus::Removed { .. })
    }
}

/// Declares a lint rule with the given metadata.
///
/// ```rust
/// use red_knot_python_semantic::declare_lint;
/// use red_knot_python_semantic::lint::{LintStatus, Level};
///
/// declare_lint! {
///     /// ## What it does
///     /// Checks for references to names that are not defined.
///     ///
///     /// ## Why is this bad?
///     /// Using an undefined variable will raise a `NameError` at runtime.
///     ///
///     /// ## Example
///     ///
///     /// ```python
///     /// print(x)  # NameError: name 'x' is not defined
///     /// ```
///     pub(crate) static UNRESOLVED_REFERENCE = {
///         summary: "detects references to names that are not defined",
///         status: LintStatus::preview("1.0.0"),
///         default_level: Level::Warn,
///     }
/// }
/// ```
#[macro_export]
macro_rules! declare_lint {
    (
        $(#[doc = $doc:literal])+
        $vis: vis static $name: ident = {
            summary: $summary: literal,
            status: $status: expr,
            // Optional properties
            $( $key:ident: $value:expr, )*
        }
    ) => {
        $( #[doc = $doc] )+
        #[allow(clippy::needless_update)]
        $vis static $name: $crate::lint::LintMetadata = $crate::lint::LintMetadata {
            name: ruff_db::diagnostic::LintName::of(ruff_macros::kebab_case!($name)),
            summary: $summary,
            raw_documentation: concat!($($doc,)+ "\n"),
            status: $status,
            source: concat!(file!(), ":", line!()),
            $( $key: $value, )*
            ..$crate::lint::lint_metadata_defaults()
        };
    };
}
