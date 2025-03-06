use itertools::Itertools;
use regex::Regex;
use rustc_hash::FxHashSet;
use std::sync::LazyLock;

use ruff_diagnostics::{AlwaysFixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::docstrings::{clean_space, leading_space};
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::analyze::visibility::is_staticmethod;
use ruff_python_trivia::textwrap::dedent;
use ruff_source_file::NewlineWithTrailingNewline;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::docstrings::sections::{SectionContext, SectionContexts, SectionKind};
use crate::docstrings::styles::SectionStyle;
use crate::docstrings::Docstring;
use crate::registry::Rule;
use crate::rules::pydocstyle::helpers::find_underline;
use crate::rules::pydocstyle::settings::Convention;

/// ## What it does
/// Checks for over-indented sections in docstrings.
///
/// ## Why is this bad?
/// This rule enforces a consistent style for docstrings with multiple
/// sections.
///
/// Multiline docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body. The convention is that all sections should use
/// consistent indentation. In each section, the header should match the
/// indentation of the docstring's opening quotes, and the body should be
/// indented one level further.
///
/// This rule is enabled when using the `numpy` and `google` conventions, and
/// disabled when using the `pep257` convention.
///
/// ## Example
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///         Args:
///             distance: Distance traveled.
///             time: Time spent traveling.
///
///     Returns:
///         Speed as distance divided by time.
///
///     Raises:
///         FasterThanLightError: If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// Use instead:
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Args:
///         distance: Distance traveled.
///         time: Time spent traveling.
///
///     Returns:
///         Speed as distance divided by time.
///
///     Raises:
///         FasterThanLightError: If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// ## Options
/// - `lint.pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[derive(ViolationMetadata)]
pub(crate) struct OverindentedSection {
    name: String,
}

impl AlwaysFixableViolation for OverindentedSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let OverindentedSection { name } = self;
        format!("Section is over-indented (\"{name}\")")
    }

    fn fix_title(&self) -> String {
        let OverindentedSection { name } = self;
        format!("Remove over-indentation from \"{name}\"")
    }
}

/// ## What it does
/// Checks for over-indented section underlines in docstrings.
///
/// ## Why is this bad?
/// This rule enforces a consistent style for multiline numpy-style docstrings,
/// and helps prevent incorrect syntax in docstrings using reStructuredText.
///
/// Multiline numpy-style docstrings are typically composed of a summary line,
/// followed by a blank line, followed by a series of sections. Each section
/// has a section header and a section body, and there should be a series of
/// underline characters in the line following the header. The underline should
/// have the same indentation as the header.
///
/// This rule enforces a consistent style for multiline numpy-style docstrings
/// with sections. If your docstring uses reStructuredText, the rule also
/// helps protect against incorrect reStructuredText syntax, which would cause
/// errors if you tried to use a tool such as Sphinx to generate documentation
/// from the docstring.
///
/// This rule is enabled when using the `numpy` convention, and disabled when
/// using the `google` or `pep257` conventions.
///
/// ## Example
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters
///         ----------
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///
///     Returns
///           -------
///     float
///         Speed as distance divided by time.
///
///     Raises
///       ------
///     FasterThanLightError
///         If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// Use instead:
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters
///     ----------
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///
///     Returns
///     -------
///     float
///         Speed as distance divided by time.
///
///     Raises
///     ------
///     FasterThanLightError
///         If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// ## Options
/// - `lint.pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
#[derive(ViolationMetadata)]
pub(crate) struct OverindentedSectionUnderline {
    name: String,
}

impl AlwaysFixableViolation for OverindentedSectionUnderline {
    #[derive_message_formats]
    fn message(&self) -> String {
        let OverindentedSectionUnderline { name } = self;
        format!("Section underline is over-indented (\"{name}\")")
    }

    fn fix_title(&self) -> String {
        let OverindentedSectionUnderline { name } = self;
        format!("Remove over-indentation from \"{name}\" underline")
    }
}

/// ## What it does
/// Checks for section headers in docstrings that do not begin with capital
/// letters.
///
/// ## Why is this bad?
/// For stylistic consistency, all section headers in a docstring should be
/// capitalized.
///
/// Multiline docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections. Each section typically has
/// a header and a body.
///
/// This rule is enabled when using the `numpy` and `google` conventions, and
/// disabled when using the `pep257` convention.
///
/// ## Example
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     args:
///         distance: Distance traveled.
///         time: Time spent traveling.
///
///     returns:
///         Speed as distance divided by time.
///
///     raises:
///         FasterThanLightError: If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// Use instead:
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Args:
///         distance: Distance traveled.
///         time: Time spent traveling.
///
///     Returns:
///         Speed as distance divided by time.
///
///     Raises:
///         FasterThanLightError: If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// ## Options
/// - `lint.pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[derive(ViolationMetadata)]
pub(crate) struct NonCapitalizedSectionName {
    name: String,
}

impl AlwaysFixableViolation for NonCapitalizedSectionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonCapitalizedSectionName { name } = self;
        format!("Section name should be properly capitalized (\"{name}\")")
    }

    fn fix_title(&self) -> String {
        let NonCapitalizedSectionName { name } = self;
        format!("Capitalize \"{name}\"")
    }
}

/// ## What it does
/// Checks for section headers in docstrings that are followed by non-newline
/// characters.
///
/// ## Why is this bad?
/// This rule enforces a consistent style for multiline numpy-style docstrings.
///
/// Multiline numpy-style docstrings are typically composed of a summary line,
/// followed by a blank line, followed by a series of sections. Each section
/// has a section header and a section body. The section header should be
/// followed by a newline, rather than by some other character (like a colon).
///
/// This rule is enabled when using the `numpy` convention, and disabled
/// when using the `google` or `pep257` conventions.
///
/// ## Example
/// ```python
/// # The `Parameters`, `Returns` and `Raises` section headers are all followed
/// # by a colon in this function's docstring:
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters:
///     -----------
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///
///     Returns:
///     --------
///     float
///         Speed as distance divided by time.
///
///     Raises:
///     -------
///     FasterThanLightError
///         If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// Use instead:
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters
///     ----------
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///
///     Returns
///     -------
///     float
///         Speed as distance divided by time.
///
///     Raises
///     ------
///     FasterThanLightError
///         If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// ## Options
/// - `lint.pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
#[derive(ViolationMetadata)]
pub(crate) struct MissingNewLineAfterSectionName {
    name: String,
}

impl AlwaysFixableViolation for MissingNewLineAfterSectionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingNewLineAfterSectionName { name } = self;
        format!("Section name should end with a newline (\"{name}\")")
    }

    fn fix_title(&self) -> String {
        let MissingNewLineAfterSectionName { name } = self;
        format!("Add newline after \"{name}\"")
    }
}

/// ## What it does
/// Checks for section headers in docstrings that are not followed by
/// underlines.
///
/// ## Why is this bad?
/// This rule enforces a consistent style for multiline numpy-style docstrings,
/// and helps prevent incorrect syntax in docstrings using reStructuredText.
///
/// Multiline numpy-style docstrings are typically composed of a summary line,
/// followed by a blank line, followed by a series of sections. Each section
/// has a section header and a section body, and the header should be followed
/// by a series of underline characters in the following line.
///
/// This rule enforces a consistent style for multiline numpy-style docstrings
/// with sections. If your docstring uses reStructuredText, the rule also
/// helps protect against incorrect reStructuredText syntax, which would cause
/// errors if you tried to use a tool such as Sphinx to generate documentation
/// from the docstring.
///
/// This rule is enabled when using the `numpy` convention, and disabled
/// when using the `google` or `pep257` conventions.
///
/// ## Example
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters
///
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///
///     Returns
///
///     float
///         Speed as distance divided by time.
///
///     Raises
///
///     FasterThanLightError
///         If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// Use instead:
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters
///     ----------
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///
///     Returns
///     -------
///     float
///         Speed as distance divided by time.
///
///     Raises
///     ------
///     FasterThanLightError
///         If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// ## Options
/// - `lint.pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
#[derive(ViolationMetadata)]
pub(crate) struct MissingDashedUnderlineAfterSection {
    name: String,
}

impl AlwaysFixableViolation for MissingDashedUnderlineAfterSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingDashedUnderlineAfterSection { name } = self;
        format!("Missing dashed underline after section (\"{name}\")")
    }

    fn fix_title(&self) -> String {
        let MissingDashedUnderlineAfterSection { name } = self;
        format!("Add dashed line under \"{name}\"")
    }
}

/// ## What it does
/// Checks for section underlines in docstrings that are not on the line
/// immediately following the section name.
///
/// ## Why is this bad?
/// This rule enforces a consistent style for multiline numpy-style docstrings,
/// and helps prevent incorrect syntax in docstrings using reStructuredText.
///
/// Multiline numpy-style docstrings are typically composed of a summary line,
/// followed by a blank line, followed by a series of sections. Each section
/// has a header and a body. There should be a series of underline characters
/// in the line immediately below the header.
///
/// This rule enforces a consistent style for multiline numpy-style docstrings
/// with sections. If your docstring uses reStructuredText, the rule also
/// helps protect against incorrect reStructuredText syntax, which would cause
/// errors if you tried to use a tool such as Sphinx to generate documentation
/// from the docstring.
///
/// This rule is enabled when using the `numpy` convention, and disabled
/// when using the `google` or `pep257` conventions.
///
/// ## Example
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters
///
///     ----------
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///
///     Returns
///
///     -------
///     float
///         Speed as distance divided by time.
///
///     Raises
///
///     ------
///     FasterThanLightError
///         If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// Use instead:
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters
///     ----------
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///
///     Returns
///     -------
///     float
///         Speed as distance divided by time.
///
///     Raises
///     ------
///     FasterThanLightError
///         If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// ## Options
/// - `lint.pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
#[derive(ViolationMetadata)]
pub(crate) struct MissingSectionUnderlineAfterName {
    name: String,
}

impl AlwaysFixableViolation for MissingSectionUnderlineAfterName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingSectionUnderlineAfterName { name } = self;
        format!("Section underline should be in the line following the section's name (\"{name}\")")
    }

    fn fix_title(&self) -> String {
        let MissingSectionUnderlineAfterName { name } = self;
        format!("Add underline to \"{name}\"")
    }
}

/// ## What it does
/// Checks for section underlines in docstrings that do not match the length of
/// the corresponding section header.
///
/// ## Why is this bad?
/// This rule enforces a consistent style for multiline numpy-style docstrings,
/// and helps prevent incorrect syntax in docstrings using reStructuredText.
///
/// Multiline numpy-style docstrings are typically composed of a summary line,
/// followed by a blank line, followed by a series of sections. Each section
/// has a section header and a section body, and there should be a series of
/// underline characters in the line following the header. The length of the
/// underline should exactly match the length of the section header.
///
/// This rule enforces a consistent style for multiline numpy-style docstrings
/// with sections. If your docstring uses reStructuredText, the rule also
/// helps protect against incorrect reStructuredText syntax, which would cause
/// errors if you tried to use a tool such as Sphinx to generate documentation
/// from the docstring.
///
/// This rule is enabled when using the `numpy` convention, and disabled
/// when using the `google` or `pep257` conventions.
///
/// ## Example
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters
///     ---
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///
///     Returns
///     ---
///     float
///         Speed as distance divided by time.
///
///     Raises
///     ---
///     FasterThanLightError
///         If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// Use instead:
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters
///     ----------
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///
///     Returns
///     -------
///     float
///         Speed as distance divided by time.
///
///     Raises
///     ------
///     FasterThanLightError
///         If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// ## Options
/// - `lint.pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
#[derive(ViolationMetadata)]
pub(crate) struct MismatchedSectionUnderlineLength {
    name: String,
}

impl AlwaysFixableViolation for MismatchedSectionUnderlineLength {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MismatchedSectionUnderlineLength { name } = self;
        format!("Section underline should match the length of its name (\"{name}\")")
    }

    fn fix_title(&self) -> String {
        let MismatchedSectionUnderlineLength { name } = self;
        format!("Adjust underline length to match \"{name}\"")
    }
}

/// ## What it does
/// Checks for docstring sections that are not separated by a single blank
/// line.
///
/// ## Why is this bad?
/// This rule enforces consistency in your docstrings, and helps ensure
/// compatibility with documentation tooling.
///
/// Multiline docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body. If a multiline numpy-style or Google-style docstring
/// consists of multiple sections, each section should be separated by a single
/// blank line.
///
/// This rule is enabled when using the `numpy` and `google` conventions, and
/// disabled when using the `pep257` convention.
///
/// ## Example
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters
///     ----------
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///     Returns
///     -------
///     float
///         Speed as distance divided by time.
///     Raises
///     ------
///     FasterThanLightError
///         If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// Use instead:
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters
///     ----------
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///
///     Returns
///     -------
///     float
///         Speed as distance divided by time.
///
///     Raises
///     ------
///     FasterThanLightError
///         If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// ## Options
/// - `lint.pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Style Guide](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[derive(ViolationMetadata)]
pub(crate) struct NoBlankLineAfterSection {
    name: String,
}

impl AlwaysFixableViolation for NoBlankLineAfterSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoBlankLineAfterSection { name } = self;
        format!("Missing blank line after section (\"{name}\")")
    }

    fn fix_title(&self) -> String {
        let NoBlankLineAfterSection { name } = self;
        format!("Add blank line after \"{name}\"")
    }
}

/// ## What it does
/// Checks for docstring sections that are not separated by a blank line.
///
/// ## Why is this bad?
/// This rule enforces consistency in numpy-style and Google-style docstrings,
/// and helps ensure compatibility with documentation tooling.
///
/// Multiline docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body. Sections should be separated by a single blank line.
///
/// This rule is enabled when using the `numpy` and `google` conventions, and
/// disabled when using the `pep257` convention.
///
/// ## Example
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters
///     ----------
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///     Returns
///     -------
///     float
///         Speed as distance divided by time.
///     Raises
///     ------
///     FasterThanLightError
///         If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// Use instead:
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters
///     ----------
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///
///     Returns
///     -------
///     float
///         Speed as distance divided by time.
///
///     Raises
///     ------
///     FasterThanLightError
///         If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// ## Options
/// - `lint.pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
#[derive(ViolationMetadata)]
pub(crate) struct NoBlankLineBeforeSection {
    name: String,
}

impl AlwaysFixableViolation for NoBlankLineBeforeSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoBlankLineBeforeSection { name } = self;
        format!("Missing blank line before section (\"{name}\")")
    }

    fn fix_title(&self) -> String {
        let NoBlankLineBeforeSection { name } = self;
        format!("Add blank line before \"{name}\"")
    }
}

/// ## What it does
/// Checks for missing blank lines after the last section of a multiline
/// docstring.
///
/// ## Why is this bad?
/// This rule enforces a consistent style for multiline docstrings.
///
/// Multiline docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body.
///
/// This rule may not apply to all projects; its applicability is a matter of
/// convention. By default, the rule is disabled when using the `google`,
/// `numpy`, and `pep257` conventions.
///
/// ## Example
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters
///     ----------
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///     Returns
///     -------
///     float
///         Speed as distance divided by time.
///     Raises
///     ------
///     FasterThanLightError
///         If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// Use instead:
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters
///     ----------
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///
///     Returns
///     -------
///     float
///         Speed as distance divided by time.
///
///     Raises
///     ------
///     FasterThanLightError
///         If speed is greater than the speed of light.
///
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// ## Options
/// - `lint.pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
#[derive(ViolationMetadata)]
pub(crate) struct MissingBlankLineAfterLastSection {
    name: String,
}

impl AlwaysFixableViolation for MissingBlankLineAfterLastSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingBlankLineAfterLastSection { name } = self;
        format!("Missing blank line after last section (\"{name}\")")
    }

    fn fix_title(&self) -> String {
        let MissingBlankLineAfterLastSection { name } = self;
        format!("Add blank line after \"{name}\"")
    }
}

/// ## What it does
/// Checks for docstrings with empty sections.
///
/// ## Why is this bad?
/// An empty section in a multiline docstring likely indicates an unfinished
/// or incomplete docstring.
///
/// Multiline docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body. Each section body should be non-empty; empty sections
/// should either have content added to them, or be removed entirely.
///
/// ## Example
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters
///     ----------
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///
///     Returns
///     -------
///     float
///         Speed as distance divided by time.
///
///     Raises
///     ------
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// Use instead:
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters
///     ----------
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///
///     Returns
///     -------
///     float
///         Speed as distance divided by time.
///
///     Raises
///     ------
///     FasterThanLightError
///         If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Style Guide](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[derive(ViolationMetadata)]
pub(crate) struct EmptyDocstringSection {
    name: String,
}

impl Violation for EmptyDocstringSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let EmptyDocstringSection { name } = self;
        format!("Section has no content (\"{name}\")")
    }
}

/// ## What it does
/// Checks for docstring section headers that do not end with a colon.
///
/// ## Why is this bad?
/// This rule enforces a consistent style for multiline Google-style
/// docstrings. If a multiline Google-style docstring consists of multiple
/// sections, each section header should end with a colon.
///
/// Multiline docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body.
///
/// This rule is enabled when using the `google` convention, and disabled when
/// using the `pep257` and `numpy` conventions.
///
/// ## Example
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Args
///         distance: Distance traveled.
///         time: Time spent traveling.
///
///     Returns
///         Speed as distance divided by time.
///
///     Raises
///         FasterThanLightError: If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// Use instead:
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Args:
///         distance: Distance traveled.
///         time: Time spent traveling.
///
///     Returns:
///         Speed as distance divided by time.
///
///     Raises:
///         FasterThanLightError: If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// ## Options
/// - `lint.pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [Google Style Guide](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[derive(ViolationMetadata)]
pub(crate) struct MissingSectionNameColon {
    name: String,
}

impl AlwaysFixableViolation for MissingSectionNameColon {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingSectionNameColon { name } = self;
        format!("Section name should end with a colon (\"{name}\")")
    }

    fn fix_title(&self) -> String {
        let MissingSectionNameColon { name } = self;
        format!("Add colon to \"{name}\"")
    }
}

/// ## What it does
/// Checks for function docstrings that do not include documentation for all
/// parameters in the function.
///
/// ## Why is this bad?
/// This rule helps prevent you from leaving Google-style docstrings unfinished
/// or incomplete. Multiline Google-style docstrings should describe all
/// parameters for the function they are documenting.
///
/// Multiline docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body. Function docstrings often include a section for
/// function arguments; this rule is concerned with that section only.
/// Note that this rule only checks docstrings with an arguments (e.g. `Args`) section.
///
/// This rule is enabled when using the `google` convention, and disabled when
/// using the `pep257` and `numpy` conventions.
///
/// ## Example
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Args:
///         distance: Distance traveled.
///
///     Returns:
///         Speed as distance divided by time.
///
///     Raises:
///         FasterThanLightError: If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// Use instead:
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Args:
///         distance: Distance traveled.
///         time: Time spent traveling.
///
///     Returns:
///         Speed as distance divided by time.
///
///     Raises:
///         FasterThanLightError: If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// ## Options
/// - `lint.pydocstyle.convention`
/// - `lint.pydocstyle.ignore-var-parameters`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[derive(ViolationMetadata)]
pub(crate) struct UndocumentedParam {
    /// The name of the function being documented.
    definition: String,
    /// The names of the undocumented parameters.
    names: Vec<String>,
}

impl Violation for UndocumentedParam {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndocumentedParam { definition, names } = self;
        if names.len() == 1 {
            let name = &names[0];
            format!("Missing argument description in the docstring for `{definition}`: `{name}`")
        } else {
            let names = names.iter().map(|name| format!("`{name}`")).join(", ");
            format!("Missing argument descriptions in the docstring for `{definition}`: {names}")
        }
    }
}

/// ## What it does
/// Checks for docstring sections that contain blank lines between a section
/// header and a section body.
///
/// ## Why is this bad?
/// This rule enforces a consistent style for multiline docstrings.
///
/// Multiline docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body. There should be no blank lines between a section header
/// and a section body.
///
/// ## Example
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Args:
///
///         distance: Distance traveled.
///         time: Time spent traveling.
///
///     Returns:
///         Speed as distance divided by time.
///
///     Raises:
///         FasterThanLightError: If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// Use instead:
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Args:
///         distance: Distance traveled.
///         time: Time spent traveling.
///
///     Returns:
///         Speed as distance divided by time.
///
///     Raises:
///         FasterThanLightError: If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[derive(ViolationMetadata)]
pub(crate) struct BlankLinesBetweenHeaderAndContent {
    name: String,
}

impl AlwaysFixableViolation for BlankLinesBetweenHeaderAndContent {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLinesBetweenHeaderAndContent { name } = self;
        format!("No blank lines allowed between a section header and its content (\"{name}\")")
    }

    fn fix_title(&self) -> String {
        "Remove blank line(s)".to_string()
    }
}

/// D212, D214, D215, D405, D406, D407, D408, D409, D410, D411, D412, D413,
/// D414, D416, D417
pub(crate) fn sections(
    checker: &Checker,
    docstring: &Docstring,
    section_contexts: &SectionContexts,
    convention: Option<Convention>,
) {
    match convention {
        Some(Convention::Google) => parse_google_sections(checker, docstring, section_contexts),
        Some(Convention::Numpy) => parse_numpy_sections(checker, docstring, section_contexts),
        Some(Convention::Pep257) | None => match section_contexts.style() {
            SectionStyle::Google => parse_google_sections(checker, docstring, section_contexts),
            SectionStyle::Numpy => parse_numpy_sections(checker, docstring, section_contexts),
        },
    }
}

fn blanks_and_section_underline(
    checker: &Checker,
    docstring: &Docstring,
    context: &SectionContext,
    style: SectionStyle,
) {
    let mut num_blank_lines_after_header = 0u32;
    let mut blank_lines_end = context.following_range().start();
    let mut following_lines = context.following_lines().peekable();

    while let Some(line) = following_lines.peek() {
        if line.trim().is_empty() {
            blank_lines_end = line.full_end();
            num_blank_lines_after_header += 1;
            following_lines.next();
        } else {
            break;
        }
    }

    if let Some(non_blank_line) = following_lines.next() {
        if let Some(dashed_line) = find_underline(&non_blank_line, '-') {
            if num_blank_lines_after_header > 0 {
                if checker.enabled(Rule::MissingSectionUnderlineAfterName) {
                    let mut diagnostic = Diagnostic::new(
                        MissingSectionUnderlineAfterName {
                            name: context.section_name().to_string(),
                        },
                        dashed_line,
                    );

                    // Delete any blank lines between the header and the underline.
                    diagnostic.set_fix(Fix::safe_edit(Edit::deletion(
                        context.following_range().start(),
                        blank_lines_end,
                    )));

                    checker.report_diagnostic(diagnostic);
                }
            }

            if dashed_line.len().to_usize() != context.section_name().len() {
                if checker.enabled(Rule::MismatchedSectionUnderlineLength) {
                    let mut diagnostic = Diagnostic::new(
                        MismatchedSectionUnderlineLength {
                            name: context.section_name().to_string(),
                        },
                        dashed_line,
                    );

                    // Replace the existing underline with a line of the appropriate length.
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        "-".repeat(context.section_name().len()),
                        dashed_line,
                    )));

                    checker.report_diagnostic(diagnostic);
                }
            }

            if checker.enabled(Rule::OverindentedSectionUnderline) {
                let leading_space = leading_space(&non_blank_line);
                let docstring_indentation = docstring.compute_indentation();
                if leading_space.len() > docstring_indentation.len() {
                    let mut diagnostic = Diagnostic::new(
                        OverindentedSectionUnderline {
                            name: context.section_name().to_string(),
                        },
                        dashed_line,
                    );

                    // Replace the existing indentation with whitespace of the appropriate length.
                    let range = TextRange::at(
                        blank_lines_end,
                        leading_space.text_len() + TextSize::from(1),
                    );
                    let contents = clean_space(docstring_indentation);
                    diagnostic.set_fix(Fix::safe_edit(if contents.is_empty() {
                        Edit::range_deletion(range)
                    } else {
                        Edit::range_replacement(contents, range)
                    }));

                    checker.report_diagnostic(diagnostic);
                }
            }

            if let Some(line_after_dashes) = following_lines.next() {
                if line_after_dashes.trim().is_empty() {
                    let mut num_blank_lines_dashes_end = 1u32;
                    let mut blank_lines_after_dashes_end = line_after_dashes.full_end();
                    while let Some(line) = following_lines.peek() {
                        if line.trim().is_empty() {
                            blank_lines_after_dashes_end = line.full_end();
                            num_blank_lines_dashes_end += 1;
                            following_lines.next();
                        } else {
                            break;
                        }
                    }

                    if following_lines.peek().is_none() {
                        if checker.enabled(Rule::EmptyDocstringSection) {
                            checker.report_diagnostic(Diagnostic::new(
                                EmptyDocstringSection {
                                    name: context.section_name().to_string(),
                                },
                                context.section_name_range(),
                            ));
                        }
                    } else if checker.enabled(Rule::BlankLinesBetweenHeaderAndContent) {
                        // If the section is followed by exactly one line, and then a
                        // reStructuredText directive, the blank lines should be preserved, as in:
                        //
                        // ```python
                        // """
                        // Example
                        // -------
                        //
                        // .. code-block:: python
                        //
                        //     import foo
                        // """
                        // ```
                        //
                        // Otherwise, documentation generators will not recognize the directive.
                        let is_sphinx = checker
                            .locator()
                            .line_str(blank_lines_after_dashes_end)
                            .trim_start()
                            .starts_with(".. ");

                        if is_sphinx {
                            if num_blank_lines_dashes_end > 1 {
                                let mut diagnostic = Diagnostic::new(
                                    BlankLinesBetweenHeaderAndContent {
                                        name: context.section_name().to_string(),
                                    },
                                    context.section_name_range(),
                                );
                                // Preserve a single blank line between the header and content.
                                diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
                                    checker.stylist().line_ending().to_string(),
                                    line_after_dashes.start(),
                                    blank_lines_after_dashes_end,
                                )));
                                checker.report_diagnostic(diagnostic);
                            }
                        } else {
                            let mut diagnostic = Diagnostic::new(
                                BlankLinesBetweenHeaderAndContent {
                                    name: context.section_name().to_string(),
                                },
                                context.section_name_range(),
                            );
                            // Delete any blank lines between the header and content.
                            diagnostic.set_fix(Fix::safe_edit(Edit::deletion(
                                line_after_dashes.start(),
                                blank_lines_after_dashes_end,
                            )));
                            checker.report_diagnostic(diagnostic);
                        }
                    }
                }
            } else {
                if checker.enabled(Rule::EmptyDocstringSection) {
                    checker.report_diagnostic(Diagnostic::new(
                        EmptyDocstringSection {
                            name: context.section_name().to_string(),
                        },
                        context.section_name_range(),
                    ));
                }
            }
        } else {
            if style.is_numpy() && checker.enabled(Rule::MissingDashedUnderlineAfterSection) {
                if let Some(equal_line) = find_underline(&non_blank_line, '=') {
                    let mut diagnostic = Diagnostic::new(
                        MissingDashedUnderlineAfterSection {
                            name: context.section_name().to_string(),
                        },
                        equal_line,
                    );

                    // If an existing underline is an equal sign line of the appropriate length,
                    // replace it with a dashed line.
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        "-".repeat(context.section_name().len()),
                        equal_line,
                    )));

                    checker.report_diagnostic(diagnostic);
                } else {
                    let mut diagnostic = Diagnostic::new(
                        MissingDashedUnderlineAfterSection {
                            name: context.section_name().to_string(),
                        },
                        context.section_name_range(),
                    );

                    // Add a dashed line (of the appropriate length) under the section header.
                    let content = format!(
                        "{}{}{}",
                        checker.stylist().line_ending().as_str(),
                        clean_space(docstring.compute_indentation()),
                        "-".repeat(context.section_name().len()),
                    );
                    diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                        content,
                        context.summary_range().end(),
                    )));

                    checker.report_diagnostic(diagnostic);
                }
            }
            if num_blank_lines_after_header > 0 {
                if checker.enabled(Rule::BlankLinesBetweenHeaderAndContent) {
                    // If the section is followed by exactly one line, and then a
                    // reStructuredText directive, the blank lines should be preserved, as in:
                    //
                    // ```python
                    // """
                    // Example:
                    //
                    // .. code-block:: python
                    //
                    //     import foo
                    // """
                    // ```
                    //
                    // Otherwise, documentation generators will not recognize the directive.
                    let is_sphinx = checker
                        .locator()
                        .line_str(blank_lines_end)
                        .trim_start()
                        .starts_with(".. ");

                    if is_sphinx {
                        if num_blank_lines_after_header > 1 {
                            let mut diagnostic = Diagnostic::new(
                                BlankLinesBetweenHeaderAndContent {
                                    name: context.section_name().to_string(),
                                },
                                context.section_name_range(),
                            );

                            // Preserve a single blank line between the header and content.
                            diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
                                checker.stylist().line_ending().to_string(),
                                context.following_range().start(),
                                blank_lines_end,
                            )));
                            checker.report_diagnostic(diagnostic);
                        }
                    } else {
                        let mut diagnostic = Diagnostic::new(
                            BlankLinesBetweenHeaderAndContent {
                                name: context.section_name().to_string(),
                            },
                            context.section_name_range(),
                        );

                        let range =
                            TextRange::new(context.following_range().start(), blank_lines_end);
                        // Delete any blank lines between the header and content.
                        diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(range)));
                        checker.report_diagnostic(diagnostic);
                    }
                }
            }
        }
    } else {
        // Nothing but blank lines after the section header.
        if style.is_numpy() && checker.enabled(Rule::MissingDashedUnderlineAfterSection) {
            let mut diagnostic = Diagnostic::new(
                MissingDashedUnderlineAfterSection {
                    name: context.section_name().to_string(),
                },
                context.section_name_range(),
            );

            // Add a dashed line (of the appropriate length) under the section header.
            let content = format!(
                "{}{}{}",
                checker.stylist().line_ending().as_str(),
                clean_space(docstring.compute_indentation()),
                "-".repeat(context.section_name().len()),
            );
            diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                content,
                context.summary_range().end(),
            )));

            checker.report_diagnostic(diagnostic);
        }
        if checker.enabled(Rule::EmptyDocstringSection) {
            checker.report_diagnostic(Diagnostic::new(
                EmptyDocstringSection {
                    name: context.section_name().to_string(),
                },
                context.section_name_range(),
            ));
        }
    }
}

fn common_section(
    checker: &Checker,
    docstring: &Docstring,
    context: &SectionContext,
    next: Option<&SectionContext>,
    style: SectionStyle,
) {
    if checker.enabled(Rule::NonCapitalizedSectionName) {
        let capitalized_section_name = context.kind().as_str();
        if context.section_name() != capitalized_section_name {
            let section_range = context.section_name_range();
            let mut diagnostic = Diagnostic::new(
                NonCapitalizedSectionName {
                    name: context.section_name().to_string(),
                },
                section_range,
            );
            // Replace the section title with the capitalized variant. This requires
            // locating the start and end of the section name.
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                capitalized_section_name.to_string(),
                section_range,
            )));
            checker.report_diagnostic(diagnostic);
        }
    }

    if checker.enabled(Rule::OverindentedSection) {
        let leading_space = leading_space(context.summary_line());
        let docstring_indentation = docstring.compute_indentation();
        if leading_space.len() > docstring_indentation.len() {
            let section_range = context.section_name_range();
            let mut diagnostic = Diagnostic::new(
                OverindentedSection {
                    name: context.section_name().to_string(),
                },
                section_range,
            );

            // Replace the existing indentation with whitespace of the appropriate length.
            let content = clean_space(docstring_indentation);
            let fix_range = TextRange::at(context.start(), leading_space.text_len());
            diagnostic.set_fix(Fix::safe_edit(if content.is_empty() {
                Edit::range_deletion(fix_range)
            } else {
                Edit::range_replacement(content, fix_range)
            }));
            checker.report_diagnostic(diagnostic);
        }
    }

    let line_end = checker.stylist().line_ending().as_str();

    if let Some(next) = next {
        if checker.enabled(Rule::NoBlankLineAfterSection) {
            let num_blank_lines = context
                .following_lines()
                .rev()
                .take_while(|line| line.trim().is_empty())
                .count();
            if num_blank_lines < 2 {
                let section_range = context.section_name_range();
                let mut diagnostic = Diagnostic::new(
                    NoBlankLineAfterSection {
                        name: context.section_name().to_string(),
                    },
                    section_range,
                );
                // Add a newline at the beginning of the next section.
                diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                    line_end.to_string(),
                    next.start(),
                )));
                checker.report_diagnostic(diagnostic);
            }
        }
    } else {
        // The first blank line is the line containing the closing triple quotes, so we need at
        // least two.
        if checker.enabled(Rule::MissingBlankLineAfterLastSection) {
            let num_blank_lines = context
                .following_lines()
                .rev()
                .take_while(|line| line.trim().is_empty())
                .count();
            if num_blank_lines < 2 {
                let del_len = if num_blank_lines == 1 {
                    // SAFETY: Guaranteed to not be None, because `num_blank_lines`is 1.
                    context.following_lines().next_back().unwrap().text_len()
                } else {
                    TextSize::new(0)
                };

                let edit = Edit::replacement(
                    format!(
                        "{}{}",
                        line_end.repeat(2 - num_blank_lines),
                        docstring.compute_indentation()
                    ),
                    context.end() - del_len,
                    context.end(),
                );

                let section_range = context.section_name_range();
                let mut diagnostic = Diagnostic::new(
                    MissingBlankLineAfterLastSection {
                        name: context.section_name().to_string(),
                    },
                    section_range,
                );
                diagnostic.set_fix(Fix::safe_edit(edit));
                checker.report_diagnostic(diagnostic);
            }
        }
    }

    if checker.enabled(Rule::NoBlankLineBeforeSection) {
        if !context
            .previous_line()
            .is_some_and(|line| line.trim().is_empty())
        {
            let section_range = context.section_name_range();
            let mut diagnostic = Diagnostic::new(
                NoBlankLineBeforeSection {
                    name: context.section_name().to_string(),
                },
                section_range,
            );
            // Add a blank line before the section.
            diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                line_end.to_string(),
                context.start(),
            )));
            checker.report_diagnostic(diagnostic);
        }
    }

    blanks_and_section_underline(checker, docstring, context, style);
}

fn missing_args(checker: &Checker, docstring: &Docstring, docstrings_args: &FxHashSet<String>) {
    let Some(function) = docstring.definition.as_function_def() else {
        return;
    };

    // Look for arguments that weren't included in the docstring.
    let mut missing_arg_names: FxHashSet<String> = FxHashSet::default();

    // If this is a non-static method, skip `cls` or `self`.
    for parameter in function
        .parameters
        .iter_non_variadic_params()
        .skip(usize::from(
            docstring.definition.is_method()
                && !is_staticmethod(&function.decorator_list, checker.semantic()),
        ))
    {
        let arg_name = parameter.name().as_str();
        if !arg_name.starts_with('_') && !docstrings_args.contains(arg_name) {
            missing_arg_names.insert(arg_name.to_string());
        }
    }

    // Check specifically for `vararg` and `kwarg`, which can be prefixed with a
    // single or double star, respectively.
    if !checker.settings.pydocstyle.ignore_var_parameters() {
        if let Some(arg) = function.parameters.vararg.as_ref() {
            let arg_name = arg.name.as_str();
            let starred_arg_name = format!("*{arg_name}");
            if !arg_name.starts_with('_')
                && !docstrings_args.contains(arg_name)
                && !docstrings_args.contains(&starred_arg_name)
            {
                missing_arg_names.insert(starred_arg_name);
            }
        }
        if let Some(arg) = function.parameters.kwarg.as_ref() {
            let arg_name = arg.name.as_str();
            let starred_arg_name = format!("**{arg_name}");
            if !arg_name.starts_with('_')
                && !docstrings_args.contains(arg_name)
                && !docstrings_args.contains(&starred_arg_name)
            {
                missing_arg_names.insert(starred_arg_name);
            }
        }
    }

    if !missing_arg_names.is_empty() {
        if let Some(definition) = docstring.definition.name() {
            let names = missing_arg_names.into_iter().sorted().collect();
            checker.report_diagnostic(Diagnostic::new(
                UndocumentedParam {
                    definition: definition.to_string(),
                    names,
                },
                function.identifier(),
            ));
        }
    }
}

// See: `GOOGLE_ARGS_REGEX` in `pydocstyle/checker.py`.
static GOOGLE_ARGS_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(\*?\*?\w+)\s*(\(.*?\))?\s*:(\r\n|\n)?\s*.+").unwrap());

fn args_section(context: &SectionContext) -> FxHashSet<String> {
    let mut following_lines = context.following_lines();
    let Some(first_line) = following_lines.next() else {
        return FxHashSet::default();
    };

    // Normalize leading whitespace, by removing any lines with less indentation
    // than the first.
    let leading_space = leading_space(first_line.as_str());
    let relevant_lines = std::iter::once(first_line)
        .chain(following_lines)
        .filter(|line| {
            line.is_empty()
                || (line.starts_with(leading_space) && find_underline(line, '-').is_none())
        })
        .map(|line| line.as_str())
        .join("\n");
    let args_content = dedent(&relevant_lines);

    // Reformat each section.
    let mut args_sections: Vec<String> = vec![];
    for line in args_content.trim().lines() {
        if line.chars().next().is_none_or(char::is_whitespace) {
            // This is a continuation of the documentation for the previous parameter,
            // because it starts with whitespace.
            if let Some(last) = args_sections.last_mut() {
                last.push_str(line);
                last.push('\n');
            }
        } else {
            // This line is the start of documentation for the next parameter, because it
            // doesn't start with any whitespace.
            let mut line = line.to_string();
            line.push('\n');
            args_sections.push(line);
        }
    }

    // Extract the argument name from each section.
    let mut matches = Vec::new();
    for section in &args_sections {
        if let Some(captures) = GOOGLE_ARGS_REGEX.captures(section) {
            matches.push(captures);
        }
    }

    matches
        .iter()
        .filter_map(|captures| captures.get(1).map(|arg_name| arg_name.as_str().to_owned()))
        .collect::<FxHashSet<String>>()
}

fn parameters_section(checker: &Checker, docstring: &Docstring, context: &SectionContext) {
    // Collect the list of arguments documented in the docstring.
    let mut docstring_args: FxHashSet<String> = FxHashSet::default();
    let section_level_indent = leading_space(context.summary_line());

    // Join line continuations, then resplit by line.
    let adjusted_following_lines = context
        .following_lines()
        .map(|l| l.as_str())
        .join("\n")
        .replace("\\\n", "");
    let mut lines = NewlineWithTrailingNewline::from(&adjusted_following_lines);
    if let Some(mut current_line) = lines.next() {
        for next_line in lines {
            let current_leading_space = leading_space(current_line.as_str());
            if current_leading_space == section_level_indent
                && (leading_space(&next_line).len() > current_leading_space.len())
                && !next_line.trim().is_empty()
            {
                let parameters = if let Some(semi_index) = current_line.find(':') {
                    // If the parameter has a type annotation, exclude it.
                    &current_line.as_str()[..semi_index]
                } else {
                    // Otherwise, it's just a list of parameters on the current line.
                    current_line.as_str().trim()
                };
                // Notably, NumPy lets you put multiple parameters of the same type on the same
                // line.
                for parameter in parameters.split(',') {
                    docstring_args.insert(parameter.trim().to_owned());
                }
            }

            current_line = next_line;
        }
    }

    // Validate that all arguments were documented.
    missing_args(checker, docstring, &docstring_args);
}

fn numpy_section(
    checker: &Checker,
    docstring: &Docstring,
    context: &SectionContext,
    next: Option<&SectionContext>,
) {
    common_section(checker, docstring, context, next, SectionStyle::Numpy);

    if checker.enabled(Rule::MissingNewLineAfterSectionName) {
        let suffix = context.summary_after_section_name();

        if !suffix.is_empty() {
            let mut diagnostic = Diagnostic::new(
                MissingNewLineAfterSectionName {
                    name: context.section_name().to_string(),
                },
                context.section_name_range(),
            );
            let section_range = context.section_name_range();
            diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(TextRange::at(
                section_range.end(),
                suffix.text_len(),
            ))));

            checker.report_diagnostic(diagnostic);
        }
    }

    if checker.enabled(Rule::UndocumentedParam) {
        if matches!(context.kind(), SectionKind::Parameters) {
            parameters_section(checker, docstring, context);
        }
    }
}

fn google_section(
    checker: &Checker,
    docstring: &Docstring,
    context: &SectionContext,
    next: Option<&SectionContext>,
) {
    common_section(checker, docstring, context, next, SectionStyle::Google);

    if checker.enabled(Rule::MissingSectionNameColon) {
        let suffix = context.summary_after_section_name();
        if suffix != ":" {
            let mut diagnostic = Diagnostic::new(
                MissingSectionNameColon {
                    name: context.section_name().to_string(),
                },
                context.section_name_range(),
            );
            // Replace the suffix.
            let section_name_range = context.section_name_range();
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                ":".to_string(),
                TextRange::at(section_name_range.end(), suffix.text_len()),
            )));
            checker.report_diagnostic(diagnostic);
        }
    }
}

fn parse_numpy_sections(
    checker: &Checker,
    docstring: &Docstring,
    section_contexts: &SectionContexts,
) {
    let mut iterator = section_contexts.iter().peekable();
    while let Some(context) = iterator.next() {
        numpy_section(checker, docstring, &context, iterator.peek());
    }
}

fn parse_google_sections(
    checker: &Checker,
    docstring: &Docstring,
    section_contexts: &SectionContexts,
) {
    let mut iterator = section_contexts.iter().peekable();
    while let Some(context) = iterator.next() {
        google_section(checker, docstring, &context, iterator.peek());
    }

    if checker.enabled(Rule::UndocumentedParam) {
        let mut has_args = false;
        let mut documented_args: FxHashSet<String> = FxHashSet::default();
        for section_context in section_contexts {
            // Checks occur at the section level. Since two sections (args/keyword args and their
            // variants) can list arguments, we need to unify the sets of arguments mentioned in both
            // then check for missing arguments at the end of the section check.
            if matches!(
                section_context.kind(),
                SectionKind::Args
                    | SectionKind::Arguments
                    | SectionKind::KeywordArgs
                    | SectionKind::KeywordArguments
                    | SectionKind::OtherArgs
                    | SectionKind::OtherArguments
            ) {
                has_args = true;
                documented_args.extend(args_section(&section_context));
            }
        }
        if has_args {
            missing_args(checker, docstring, &documented_args);
        }
    }
}
