use itertools::Itertools;
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::docstrings::sections::{SectionContext, SectionContexts, SectionKind};
use crate::docstrings::Docstring;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks that sections in a numpydoc docstring are in the correct order.
///
/// ## Why is this bad?
/// Numpydoc style guidelines require that docstring sections appear in a specific
/// order to maintain consistency across documentation. Out-of-order sections can
/// make documentation harder to read and maintain.
///
/// ## Example
/// ```python
/// def foo(a: float) -> float:
///     """Calculate b.
///
///     Returns
///     -------
///     float
///         Speed as distance divided by time.
///
///     Parameters
///     ----------
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///     """
///     return distance / time
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
///     """
///     return distance / time
/// ```
///
/// ## References
/// - [Numpydoc Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Numpydoc Validation](https://numpydoc.readthedocs.io/en/latest/validation.html#built-in-validation-checks)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "1.0.0")]
pub(crate) struct WrongSectionOrder {
    section_name: String,
    expected_order: Vec<String>,
}

impl AlwaysFixableViolation for WrongSectionOrder {
    #[derive_message_formats]
    fn message(&self) -> String {
        let WrongSectionOrder { section_name, expected_order } = self;
        let expected_sections = expected_order.join(", ");
        format!(
            "Sections are in the wrong order (\"{section_name}\" out of place). Correct order is: {expected_sections}"
        )
    }

    fn fix_title(&self) -> String {
        "Reorder sections to match numpydoc convention".to_string()
    }
}

/// Returns the expected position of a section kind in numpydoc order.
fn expected_position(kind: SectionKind) -> Option<usize> {
    match kind {
        SectionKind::ShortSummary => Some(0),
        // Note: DeprecationWarning is not a SectionKind yet, it would be position 1
        SectionKind::ExtendedSummary => Some(2),
        SectionKind::Parameters => Some(3),
        SectionKind::Returns => Some(4),
        SectionKind::Yields => Some(5),
        SectionKind::Receives => Some(6),
        SectionKind::OtherParameters | SectionKind::OtherParams => Some(7),
        SectionKind::Raises => Some(8),
        SectionKind::Warns => Some(9),
        SectionKind::Warnings => Some(10),
        SectionKind::SeeAlso => Some(11),
        SectionKind::Notes => Some(12),
        SectionKind::References => Some(13),
        SectionKind::Examples => Some(14),
        _ => None,
    }
}

/// NPD007
pub(crate) fn wrong_section_order(
    checker: &Checker,
    docstring: &Docstring,
    section_contexts: &SectionContexts,
) {
    let sections: Vec<(SectionContext, usize)> = section_contexts
        .iter()
        .filter_map(|context| {
            expected_position(context.kind()).map(|pos| (context, pos))
        })
        .collect();

    // Find the first out-of-order section
    let Some(window) = sections
        .windows(2)
        .find(|window| window[0].1 > window[1].1)
    else {
        return; // All sections are in correct order
    };

    let out_of_order_section = &window[0].0;

    let expected_order: Vec<String> = sections
        .iter()
        .map(|(ctx, _)| ctx.kind().as_str().to_string())
        .sorted_by_key(|name| {
            SectionKind::from_str(name)
                .and_then(expected_position)
                .unwrap_or(usize::MAX)
        })
        .collect();

    let mut diagnostic = checker.report_diagnostic(
        WrongSectionOrder {
            section_name: out_of_order_section.section_name().to_string(),
            expected_order,
        },
        out_of_order_section.section_name_range(),
    );

    if let Some(fix) = generate_fix(docstring, &sections) {
        diagnostic.set_fix(fix);
    }
}

/// Generate a fix that reorders the sections in the correct order.
fn generate_fix(
    docstring: &Docstring,
    sections: &[(SectionContext, usize)],
) -> Option<Fix> {
    let body = docstring.body();
    let section_text = |ctx: &SectionContext| {
        let start = (ctx.range().start() - body.start()).to_usize();
        let end = (ctx.range().end() - body.start()).to_usize();
        &body.as_str()[start..end]
    };

    let (last_section_idx, trailing_whitespace) = {
        let (idx, (ctx, _)) = sections
            .iter()
            .enumerate()
            .max_by_key(|(_, (ctx, _))| ctx.range().end())?;

        let text = section_text(ctx);
        (idx, &text[text.trim_end().len()..])
    };

    let replacement = sections
        .iter()
        .map(|(ctx, expected_pos)| (section_text(ctx).trim_end(), *expected_pos))
        .sorted_by_key(|(_, pos)| *pos)
        .map(|(text, _)| text)
        .join("\n\n")
        + trailing_whitespace;

    Some(Fix::safe_edit(Edit::replacement(
        replacement,
        sections[0].0.range().start(),
        sections[last_section_idx].0.range().end(),
    )))
}
