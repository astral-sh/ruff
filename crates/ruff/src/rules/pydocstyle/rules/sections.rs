use itertools::Itertools;
use once_cell::sync::Lazy;
use regex::Regex;
use rustc_hash::FxHashSet;

use ruff_diagnostics::{AlwaysAutofixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::docstrings::{clean_space, leading_space};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::ParameterWithDefault;
use ruff_python_semantic::analyze::visibility::is_staticmethod;
use ruff_python_trivia::{textwrap::dedent, PythonWhitespace};
use ruff_source_file::NewlineWithTrailingNewline;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::docstrings::sections::{SectionContext, SectionContexts, SectionKind};
use crate::docstrings::styles::SectionStyle;
use crate::docstrings::Docstring;
use crate::registry::{AsRule, Rule};
use crate::rules::pydocstyle::settings::Convention;

/// ## What it does
/// Checks for over-indented sections in docstrings.
///
/// ## Why is this bad?
/// Multi-line docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body.
///
/// Each section should use consistent indentation, with the section headers
/// matching the indentation of the docstring's opening quotes, and the
/// section bodies being indented one level further.
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
/// - `pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[violation]
pub struct SectionNotOverIndented {
    name: String,
}

impl AlwaysAutofixableViolation for SectionNotOverIndented {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SectionNotOverIndented { name } = self;
        format!("Section is over-indented (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionNotOverIndented { name } = self;
        format!("Remove over-indentation from \"{name}\"")
    }
}

/// ## What it does
/// Checks for over-indented section underlines in docstrings.
///
/// ## Why is this bad?
/// Multi-line docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body.
///
/// Some docstring formats (like reStructuredText) use underlines to separate
/// section bodies from section headers.
///
/// Avoid over-indenting the section underlines, as this can cause syntax
/// errors in reStructuredText.
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
/// - `pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
#[violation]
pub struct SectionUnderlineNotOverIndented {
    name: String,
}

impl AlwaysAutofixableViolation for SectionUnderlineNotOverIndented {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SectionUnderlineNotOverIndented { name } = self;
        format!("Section underline is over-indented (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionUnderlineNotOverIndented { name } = self;
        format!("Remove over-indentation from \"{name}\" underline")
    }
}

/// ## What it does
/// Checks for section headers in docstrings that do not begin with capital
/// letters.
///
/// ## Why is this bad?
/// Multi-line docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body.
///
/// Section headers should be capitalized, for consistency.
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
/// - `pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[violation]
pub struct CapitalizeSectionName {
    name: String,
}

impl AlwaysAutofixableViolation for CapitalizeSectionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CapitalizeSectionName { name } = self;
        format!("Section name should be properly capitalized (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let CapitalizeSectionName { name } = self;
        format!("Capitalize \"{name}\"")
    }
}

/// ## What it does
/// Checks that section headers in docstrings that are not followed by a
/// newline.
///
/// ## Why is this bad?
/// Multi-line docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body.
///
/// Section headers should be followed by a newline, and not by another
/// character (like a colon), for consistency.
///
/// This rule is enabled when using the `numpy` convention, and disabled
/// when using the `google` or `pep257` conventions.
///
/// ## Example
/// ```python
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
/// - `pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
#[violation]
pub struct NewLineAfterSectionName {
    name: String,
}

impl AlwaysAutofixableViolation for NewLineAfterSectionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NewLineAfterSectionName { name } = self;
        format!("Section name should end with a newline (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let NewLineAfterSectionName { name } = self;
        format!("Add newline after \"{name}\"")
    }
}

/// ## What it does
/// Checks for section headers in docstrings that are not followed by
/// underlines.
///
/// ## Why is this bad?
/// Multi-line docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body.
///
/// Some docstring formats (like reStructuredText) use underlines to separate
/// section bodies from section headers.
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
/// - `pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
#[violation]
pub struct DashedUnderlineAfterSection {
    name: String,
}

impl AlwaysAutofixableViolation for DashedUnderlineAfterSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DashedUnderlineAfterSection { name } = self;
        format!("Missing dashed underline after section (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let DashedUnderlineAfterSection { name } = self;
        format!("Add dashed line under \"{name}\"")
    }
}

/// ## What it does
/// Checks for section underlines in docstrings that are not on the line
/// immediately following the section name.
///
/// ## Why is this bad?
/// Multi-line docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body.
///
/// Some docstring formats (like reStructuredText) use underlines to separate
/// section bodies from section headers.
///
/// When present, section underlines should be positioned on the line
/// immediately following the section header.
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
/// - `pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
#[violation]
pub struct SectionUnderlineAfterName {
    name: String,
}

impl AlwaysAutofixableViolation for SectionUnderlineAfterName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SectionUnderlineAfterName { name } = self;
        format!("Section underline should be in the line following the section's name (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionUnderlineAfterName { name } = self;
        format!("Add underline to \"{name}\"")
    }
}

/// ## What it does
/// Checks for section underlines in docstrings that do not match the length of
/// the corresponding section header.
///
/// ## Why is this bad?
/// Multi-line docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body.
///
/// Some docstring formats (like reStructuredText) use underlines to separate
/// section bodies from section headers.
///
/// When present, section underlines should match the length of the
/// corresponding section header.
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
/// - `pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
#[violation]
pub struct SectionUnderlineMatchesSectionLength {
    name: String,
}

impl AlwaysAutofixableViolation for SectionUnderlineMatchesSectionLength {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SectionUnderlineMatchesSectionLength { name } = self;
        format!("Section underline should match the length of its name (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionUnderlineMatchesSectionLength { name } = self;
        format!("Adjust underline length to match \"{name}\"")
    }
}

/// ## What it does
/// Checks for docstring sections that are not separated by a single blank
/// line.
///
/// ## Why is this bad?
/// Multi-line docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body.
///
/// Docstring sections should be separated by a blank line, for consistency and
/// compatibility with documentation tooling.
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
/// - `pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Style Guide](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[violation]
pub struct NoBlankLineAfterSection {
    name: String,
}

impl AlwaysAutofixableViolation for NoBlankLineAfterSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoBlankLineAfterSection { name } = self;
        format!("Missing blank line after section (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let NoBlankLineAfterSection { name } = self;
        format!("Add blank line after \"{name}\"")
    }
}

/// ## What it does
/// Checks for docstring sections that are separated by a blank line.
///
/// ## Why is this bad?
/// Multi-line docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body.
///
/// Docstring sections should be separated by a blank line, for consistency and
/// compatibility with documentation tooling.
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
/// - `pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
#[violation]
pub struct NoBlankLineBeforeSection {
    name: String,
}

impl AlwaysAutofixableViolation for NoBlankLineBeforeSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoBlankLineBeforeSection { name } = self;
        format!("Missing blank line before section (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let NoBlankLineBeforeSection { name } = self;
        format!("Add blank line before \"{name}\"")
    }
}

/// ## What it does
/// Checks for missing blank lines after the last section of a multi-line
/// docstring.
///
/// ## Why is this bad?
/// Multi-line docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body.
///
/// In some projects, the last section of a docstring is followed by a blank line,
/// for consistency and compatibility.
///
/// This rule may not apply to all projects; its applicability is a matter of
/// convention. By default, this rule is disabled when using the `google`,
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
/// - `pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
#[violation]
pub struct BlankLineAfterLastSection {
    name: String,
}

impl AlwaysAutofixableViolation for BlankLineAfterLastSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLineAfterLastSection { name } = self;
        format!("Missing blank line after last section (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let BlankLineAfterLastSection { name } = self;
        format!("Add blank line after \"{name}\"")
    }
}

/// ## What it does
/// Checks for docstrings that contain empty sections.
///
/// ## Why is this bad?
/// Multi-line docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body.
///
/// Empty docstring sections are indicative of missing documentation. Empty
/// sections should either be removed or filled in with relevant documentation.
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
/// ## Options
/// - `pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Style Guide](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[violation]
pub struct EmptyDocstringSection {
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
/// Multi-line docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body.
///
/// In a docstring, each section header should end with a colon, for
/// consistency.
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
/// - `pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [Google Style Guide](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[violation]
pub struct SectionNameEndsInColon {
    name: String,
}

impl AlwaysAutofixableViolation for SectionNameEndsInColon {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SectionNameEndsInColon { name } = self;
        format!("Section name should end with a colon (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionNameEndsInColon { name } = self;
        format!("Add colon to \"{name}\"")
    }
}

/// ## What it does
/// Checks for function docstrings that do not include documentation for all
/// parameters in the function.
///
/// ## Why is this bad?
/// Multi-line docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body.
///
/// Function docstrings often include a section for function arguments, which
/// should include documentation for every argument. Undocumented arguments are
/// indicative of missing documentation.
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
/// - `pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[violation]
pub struct UndocumentedParam {
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
/// Checks for docstring sections that contain blank lines between the section
/// header and the section body.
///
/// ## Why is this bad?
/// Multi-line docstrings are typically composed of a summary line, followed by
/// a blank line, followed by a series of sections, each with a section header
/// and a section body.
///
/// Docstring sections should not contain blank lines between the section header
/// and the section body, for consistency.
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
/// ## Options
/// - `pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[violation]
pub struct BlankLinesBetweenHeaderAndContent {
    name: String,
}

impl AlwaysAutofixableViolation for BlankLinesBetweenHeaderAndContent {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLinesBetweenHeaderAndContent { name } = self;
        format!("No blank lines allowed between a section header and its content (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        "Remove blank line(s)".to_string()
    }
}

/// D212, D214, D215, D405, D406, D407, D408, D409, D410, D411, D412, D413,
/// D414, D416, D417
pub(crate) fn sections(
    checker: &mut Checker,
    docstring: &Docstring,
    convention: Option<&Convention>,
) {
    match convention {
        Some(Convention::Google) => {
            parse_google_sections(
                checker,
                docstring,
                &SectionContexts::from_docstring(docstring, SectionStyle::Google),
            );
        }
        Some(Convention::Numpy) => {
            parse_numpy_sections(
                checker,
                docstring,
                &SectionContexts::from_docstring(docstring, SectionStyle::Numpy),
            );
        }
        Some(Convention::Pep257) | None => {
            // There are some overlapping section names, between the Google and NumPy conventions
            // (e.g., "Returns", "Raises"). Break ties by checking for the presence of some of the
            // section names that are unique to each convention.

            // If the docstring contains `Parameters:` or `Other Parameters:`, use the NumPy
            // convention.
            let numpy_sections = SectionContexts::from_docstring(docstring, SectionStyle::Numpy);
            if numpy_sections.iter().any(|context| {
                matches!(
                    context.kind(),
                    SectionKind::Parameters
                        | SectionKind::OtherParams
                        | SectionKind::OtherParameters
                )
            }) {
                parse_numpy_sections(checker, docstring, &numpy_sections);
                return;
            }

            // If the docstring contains any argument specifier, use the Google convention.
            let google_sections = SectionContexts::from_docstring(docstring, SectionStyle::Google);
            if google_sections.iter().any(|context| {
                matches!(
                    context.kind(),
                    SectionKind::Args
                        | SectionKind::Arguments
                        | SectionKind::KeywordArgs
                        | SectionKind::KeywordArguments
                        | SectionKind::OtherArgs
                        | SectionKind::OtherArguments
                )
            }) {
                parse_google_sections(checker, docstring, &google_sections);
                return;
            }

            // Otherwise, use whichever convention matched more sections.
            if google_sections.len() > numpy_sections.len() {
                parse_google_sections(checker, docstring, &google_sections);
            } else {
                parse_numpy_sections(checker, docstring, &numpy_sections);
            }
        }
    }
}

fn blanks_and_section_underline(
    checker: &mut Checker,
    docstring: &Docstring,
    context: &SectionContext,
) {
    let mut blank_lines_after_header = 0;
    let mut blank_lines_end = context.following_range().start();
    let mut following_lines = context.following_lines().peekable();

    while let Some(line) = following_lines.peek() {
        if line.trim().is_empty() {
            blank_lines_end = line.full_end();
            blank_lines_after_header += 1;
            following_lines.next();
        } else {
            break;
        }
    }

    if let Some(non_blank_line) = following_lines.next() {
        let dash_line_found = is_dashed_underline(&non_blank_line);

        if dash_line_found {
            if blank_lines_after_header > 0 {
                if checker.enabled(Rule::SectionUnderlineAfterName) {
                    let mut diagnostic = Diagnostic::new(
                        SectionUnderlineAfterName {
                            name: context.section_name().to_string(),
                        },
                        docstring.range(),
                    );
                    if checker.patch(diagnostic.kind.rule()) {
                        let range =
                            TextRange::new(context.following_range().start(), blank_lines_end);
                        // Delete any blank lines between the header and the underline.
                        diagnostic.set_fix(Fix::automatic(Edit::range_deletion(range)));
                    }
                    checker.diagnostics.push(diagnostic);
                }
            }

            if non_blank_line
                .trim()
                .chars()
                .filter(|char| *char == '-')
                .count()
                != context.section_name().len()
            {
                if checker.enabled(Rule::SectionUnderlineMatchesSectionLength) {
                    let mut diagnostic = Diagnostic::new(
                        SectionUnderlineMatchesSectionLength {
                            name: context.section_name().to_string(),
                        },
                        docstring.range(),
                    );
                    if checker.patch(diagnostic.kind.rule()) {
                        // Replace the existing underline with a line of the appropriate length.
                        let content = format!(
                            "{}{}{}",
                            clean_space(docstring.indentation),
                            "-".repeat(context.section_name().len()),
                            checker.stylist().line_ending().as_str()
                        );
                        diagnostic.set_fix(Fix::automatic(Edit::replacement(
                            content,
                            blank_lines_end,
                            non_blank_line.full_end(),
                        )));
                    };
                    checker.diagnostics.push(diagnostic);
                }
            }

            if checker.enabled(Rule::SectionUnderlineNotOverIndented) {
                let leading_space = leading_space(&non_blank_line);
                if leading_space.len() > docstring.indentation.len() {
                    let mut diagnostic = Diagnostic::new(
                        SectionUnderlineNotOverIndented {
                            name: context.section_name().to_string(),
                        },
                        docstring.range(),
                    );
                    if checker.patch(diagnostic.kind.rule()) {
                        let range = TextRange::at(
                            blank_lines_end,
                            leading_space.text_len() + TextSize::from(1),
                        );

                        // Replace the existing indentation with whitespace of the appropriate length.
                        diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                            clean_space(docstring.indentation),
                            range,
                        )));
                    };
                    checker.diagnostics.push(diagnostic);
                }
            }

            if let Some(line_after_dashes) = following_lines.next() {
                if line_after_dashes.trim().is_empty() {
                    let mut blank_lines_after_dashes_end = line_after_dashes.full_end();
                    while let Some(line) = following_lines.peek() {
                        if line.trim().is_empty() {
                            blank_lines_after_dashes_end = line.full_end();
                            following_lines.next();
                        } else {
                            break;
                        }
                    }

                    if following_lines.peek().is_none() {
                        if checker.enabled(Rule::EmptyDocstringSection) {
                            checker.diagnostics.push(Diagnostic::new(
                                EmptyDocstringSection {
                                    name: context.section_name().to_string(),
                                },
                                docstring.range(),
                            ));
                        }
                    } else if checker.enabled(Rule::BlankLinesBetweenHeaderAndContent) {
                        let mut diagnostic = Diagnostic::new(
                            BlankLinesBetweenHeaderAndContent {
                                name: context.section_name().to_string(),
                            },
                            docstring.range(),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            // Delete any blank lines between the header and content.
                            diagnostic.set_fix(Fix::automatic(Edit::deletion(
                                line_after_dashes.start(),
                                blank_lines_after_dashes_end,
                            )));
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                }
            } else {
                if checker.enabled(Rule::EmptyDocstringSection) {
                    checker.diagnostics.push(Diagnostic::new(
                        EmptyDocstringSection {
                            name: context.section_name().to_string(),
                        },
                        docstring.range(),
                    ));
                }
            }
        } else {
            let equal_line_found = non_blank_line
                .chars()
                .all(|char| char.is_whitespace() || char == '=');

            if checker.enabled(Rule::DashedUnderlineAfterSection) {
                let mut diagnostic = Diagnostic::new(
                    DashedUnderlineAfterSection {
                        name: context.section_name().to_string(),
                    },
                    docstring.range(),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // Add a dashed line (of the appropriate length) under the section header.
                    let content = format!(
                        "{}{}{}",
                        checker.stylist().line_ending().as_str(),
                        clean_space(docstring.indentation),
                        "-".repeat(context.section_name().len()),
                    );
                    if equal_line_found
                        && non_blank_line.trim_whitespace().len() == context.section_name().len()
                    {
                        // If an existing underline is an equal sign line of the appropriate length,
                        // replace it with a dashed line.
                        diagnostic.set_fix(Fix::automatic(Edit::replacement(
                            content,
                            context.summary_range().end(),
                            non_blank_line.end(),
                        )));
                    } else {
                        // Otherwise, insert a dashed line after the section header.
                        diagnostic.set_fix(Fix::automatic(Edit::insertion(
                            content,
                            context.summary_range().end(),
                        )));
                    }
                }
                checker.diagnostics.push(diagnostic);
            }
            if blank_lines_after_header > 0 {
                if checker.enabled(Rule::BlankLinesBetweenHeaderAndContent) {
                    let mut diagnostic = Diagnostic::new(
                        BlankLinesBetweenHeaderAndContent {
                            name: context.section_name().to_string(),
                        },
                        docstring.range(),
                    );
                    if checker.patch(diagnostic.kind.rule()) {
                        let range =
                            TextRange::new(context.following_range().start(), blank_lines_end);
                        // Delete any blank lines between the header and content.
                        diagnostic.set_fix(Fix::automatic(Edit::range_deletion(range)));
                    }
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
    }
    // Nothing but blank lines after the section header.
    else {
        if checker.enabled(Rule::DashedUnderlineAfterSection) {
            let mut diagnostic = Diagnostic::new(
                DashedUnderlineAfterSection {
                    name: context.section_name().to_string(),
                },
                docstring.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Add a dashed line (of the appropriate length) under the section header.
                let content = format!(
                    "{}{}{}",
                    checker.stylist().line_ending().as_str(),
                    clean_space(docstring.indentation),
                    "-".repeat(context.section_name().len()),
                );

                diagnostic.set_fix(Fix::automatic(Edit::insertion(
                    content,
                    context.summary_range().end(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
        if checker.enabled(Rule::EmptyDocstringSection) {
            checker.diagnostics.push(Diagnostic::new(
                EmptyDocstringSection {
                    name: context.section_name().to_string(),
                },
                docstring.range(),
            ));
        }
    }
}

fn common_section(
    checker: &mut Checker,
    docstring: &Docstring,
    context: &SectionContext,
    next: Option<&SectionContext>,
) {
    if checker.enabled(Rule::CapitalizeSectionName) {
        let capitalized_section_name = context.kind().as_str();
        if context.section_name() != capitalized_section_name {
            let mut diagnostic = Diagnostic::new(
                CapitalizeSectionName {
                    name: context.section_name().to_string(),
                },
                docstring.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Replace the section title with the capitalized variant. This requires
                // locating the start and end of the section name.
                let section_range = context.section_name_range();
                diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                    capitalized_section_name.to_string(),
                    section_range,
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }

    if checker.enabled(Rule::SectionNotOverIndented) {
        let leading_space = leading_space(context.summary_line());
        if leading_space.len() > docstring.indentation.len() {
            let mut diagnostic = Diagnostic::new(
                SectionNotOverIndented {
                    name: context.section_name().to_string(),
                },
                docstring.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Replace the existing indentation with whitespace of the appropriate length.
                let content = clean_space(docstring.indentation);
                let fix_range = TextRange::at(context.start(), leading_space.text_len());

                diagnostic.set_fix(Fix::automatic(if content.is_empty() {
                    Edit::range_deletion(fix_range)
                } else {
                    Edit::range_replacement(content, fix_range)
                }));
            };
            checker.diagnostics.push(diagnostic);
        }
    }

    let line_end = checker.stylist().line_ending().as_str();
    let last_line = context.following_lines().last();
    if last_line.map_or(true, |line| !line.trim().is_empty()) {
        if let Some(next) = next {
            if checker.enabled(Rule::NoBlankLineAfterSection) {
                let mut diagnostic = Diagnostic::new(
                    NoBlankLineAfterSection {
                        name: context.section_name().to_string(),
                    },
                    docstring.range(),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // Add a newline at the beginning of the next section.
                    diagnostic.set_fix(Fix::automatic(Edit::insertion(
                        line_end.to_string(),
                        next.start(),
                    )));
                }
                checker.diagnostics.push(diagnostic);
            }
        } else {
            if checker.enabled(Rule::BlankLineAfterLastSection) {
                let mut diagnostic = Diagnostic::new(
                    BlankLineAfterLastSection {
                        name: context.section_name().to_string(),
                    },
                    docstring.range(),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // Add a newline after the section.
                    diagnostic.set_fix(Fix::automatic(Edit::insertion(
                        format!("{}{}", line_end, docstring.indentation),
                        context.end(),
                    )));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }

    if checker.enabled(Rule::NoBlankLineBeforeSection) {
        if !context
            .previous_line()
            .is_some_and(|line| line.trim().is_empty())
        {
            let mut diagnostic = Diagnostic::new(
                NoBlankLineBeforeSection {
                    name: context.section_name().to_string(),
                },
                docstring.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Add a blank line before the section.
                diagnostic.set_fix(Fix::automatic(Edit::insertion(
                    line_end.to_string(),
                    context.start(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }

    blanks_and_section_underline(checker, docstring, context);
}

fn missing_args(checker: &mut Checker, docstring: &Docstring, docstrings_args: &FxHashSet<String>) {
    let Some(function) = docstring.definition.as_function_def() else {
        return;
    };

    // Look for arguments that weren't included in the docstring.
    let mut missing_arg_names: FxHashSet<String> = FxHashSet::default();
    for ParameterWithDefault {
        parameter,
        default: _,
        range: _,
    } in function
        .parameters
        .posonlyargs
        .iter()
        .chain(&function.parameters.args)
        .chain(&function.parameters.kwonlyargs)
        .skip(
            // If this is a non-static method, skip `cls` or `self`.
            usize::from(
                docstring.definition.is_method()
                    && !is_staticmethod(&function.decorator_list, checker.semantic()),
            ),
        )
    {
        let arg_name = parameter.name.as_str();
        if !arg_name.starts_with('_') && !docstrings_args.contains(arg_name) {
            missing_arg_names.insert(arg_name.to_string());
        }
    }

    // Check specifically for `vararg` and `kwarg`, which can be prefixed with a
    // single or double star, respectively.
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

    if !missing_arg_names.is_empty() {
        if let Some(definition) = docstring.definition.name() {
            let names = missing_arg_names.into_iter().sorted().collect();
            checker.diagnostics.push(Diagnostic::new(
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
static GOOGLE_ARGS_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(\*?\*?\w+)\s*(\(.*?\))?\s*:(\r\n|\n)?\s*.+").unwrap());

fn args_section(context: &SectionContext) -> FxHashSet<String> {
    let mut following_lines = context.following_lines().peekable();
    let Some(first_line) = following_lines.next() else {
        return FxHashSet::default();
    };

    // Normalize leading whitespace, by removing any lines with less indentation
    // than the first.
    let leading_space = leading_space(first_line.as_str());
    let relevant_lines = std::iter::once(first_line)
        .chain(following_lines)
        .map(|l| l.as_str())
        .filter(|line| {
            line.is_empty() || (line.starts_with(leading_space) && !is_dashed_underline(line))
        })
        .join("\n");
    let args_content = dedent(&relevant_lines);

    // Reformat each section.
    let mut args_sections: Vec<String> = vec![];
    for line in args_content.trim().lines() {
        if line.chars().next().map_or(true, char::is_whitespace) {
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

fn parameters_section(checker: &mut Checker, docstring: &Docstring, context: &SectionContext) {
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
    checker: &mut Checker,
    docstring: &Docstring,
    context: &SectionContext,
    next: Option<&SectionContext>,
) {
    common_section(checker, docstring, context, next);

    if checker.enabled(Rule::NewLineAfterSectionName) {
        let suffix = context.summary_after_section_name();

        if !suffix.is_empty() {
            let mut diagnostic = Diagnostic::new(
                NewLineAfterSectionName {
                    name: context.section_name().to_string(),
                },
                docstring.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                let section_range = context.section_name_range();
                diagnostic.set_fix(Fix::automatic(Edit::range_deletion(TextRange::at(
                    section_range.end(),
                    suffix.text_len(),
                ))));
            }

            checker.diagnostics.push(diagnostic);
        }
    }

    if checker.enabled(Rule::UndocumentedParam) {
        if matches!(context.kind(), SectionKind::Parameters) {
            parameters_section(checker, docstring, context);
        }
    }
}

fn google_section(
    checker: &mut Checker,
    docstring: &Docstring,
    context: &SectionContext,
    next: Option<&SectionContext>,
) {
    common_section(checker, docstring, context, next);

    if checker.enabled(Rule::SectionNameEndsInColon) {
        let suffix = context.summary_after_section_name();
        if suffix != ":" {
            let mut diagnostic = Diagnostic::new(
                SectionNameEndsInColon {
                    name: context.section_name().to_string(),
                },
                docstring.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Replace the suffix.
                let section_name_range = context.section_name_range();
                diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                    ":".to_string(),
                    TextRange::at(section_name_range.end(), suffix.text_len()),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

fn parse_numpy_sections(
    checker: &mut Checker,
    docstring: &Docstring,
    section_contexts: &SectionContexts,
) {
    let mut iterator = section_contexts.iter().peekable();
    while let Some(context) = iterator.next() {
        numpy_section(checker, docstring, &context, iterator.peek());
    }
}

fn parse_google_sections(
    checker: &mut Checker,
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

fn is_dashed_underline(line: &str) -> bool {
    let trimmed_line = line.trim();
    !trimmed_line.is_empty() && trimmed_line.chars().all(|char| char == '-')
}
