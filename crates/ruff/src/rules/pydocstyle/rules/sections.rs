use itertools::Itertools;
use once_cell::sync::Lazy;
use regex::Regex;
use ruff_macros::{define_violation, derive_message_formats};
use rustc_hash::FxHashSet;
use rustpython_parser::ast::StmtKind;

use crate::ast::types::Range;
use crate::ast::whitespace::LinesWithTrailingNewline;
use crate::ast::{cast, whitespace};
use crate::checkers::ast::Checker;
use crate::docstrings::definition::{DefinitionKind, Docstring};
use crate::docstrings::sections::{section_contexts, SectionContext};
use crate::docstrings::styles::SectionStyle;
use crate::fix::Fix;
use crate::message::Location;
use crate::registry::{Diagnostic, Rule};
use crate::rules::pydocstyle::settings::Convention;
use crate::violation::{AlwaysAutofixableViolation, Violation};
use crate::visibility::is_staticmethod;

define_violation!(
    pub struct SectionNotOverIndented {
        pub name: String,
    }
);
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

define_violation!(
    pub struct SectionUnderlineNotOverIndented {
        pub name: String,
    }
);
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

define_violation!(
    pub struct CapitalizeSectionName {
        pub name: String,
    }
);
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

define_violation!(
    pub struct NewLineAfterSectionName {
        pub name: String,
    }
);
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

define_violation!(
    pub struct DashedUnderlineAfterSection {
        pub name: String,
    }
);
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

define_violation!(
    pub struct SectionUnderlineAfterName {
        pub name: String,
    }
);
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

define_violation!(
    pub struct SectionUnderlineMatchesSectionLength {
        pub name: String,
    }
);
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

define_violation!(
    pub struct BlankLineAfterSection {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for BlankLineAfterSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLineAfterSection { name } = self;
        format!("Missing blank line after section (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let BlankLineAfterSection { name } = self;
        format!("Add blank line after \"{name}\"")
    }
}

define_violation!(
    pub struct BlankLineBeforeSection {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for BlankLineBeforeSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLineBeforeSection { name } = self;
        format!("Missing blank line before section (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let BlankLineBeforeSection { name } = self;
        format!("Add blank line before \"{name}\"")
    }
}

define_violation!(
    pub struct BlankLineAfterLastSection {
        pub name: String,
    }
);
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

define_violation!(
    pub struct NonEmptySection {
        pub name: String,
    }
);
impl Violation for NonEmptySection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonEmptySection { name } = self;
        format!("Section has no content (\"{name}\")")
    }
}

define_violation!(
    pub struct SectionNameEndsInColon {
        pub name: String,
    }
);
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

define_violation!(
    pub struct DocumentAllArguments {
        pub names: Vec<String>,
    }
);
impl Violation for DocumentAllArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DocumentAllArguments { names } = self;
        if names.len() == 1 {
            let name = &names[0];
            format!("Missing argument description in the docstring: `{name}`")
        } else {
            let names = names.iter().map(|name| format!("`{name}`")).join(", ");
            format!("Missing argument descriptions in the docstring: {names}")
        }
    }
}

define_violation!(
    pub struct NoBlankLinesBetweenHeaderAndContent {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for NoBlankLinesBetweenHeaderAndContent {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoBlankLinesBetweenHeaderAndContent { name } = self;
        format!("No blank lines allowed between a section header and its content (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        "Remove blank line(s)".to_string()
    }
}

/// D212, D214, D215, D405, D406, D407, D408, D409, D410, D411, D412, D413,
/// D414, D416, D417
pub fn sections(checker: &mut Checker, docstring: &Docstring, convention: Option<&Convention>) {
    let body = docstring.body;

    let lines: Vec<&str> = LinesWithTrailingNewline::from(body).collect();
    if lines.len() < 2 {
        return;
    }

    match convention {
        Some(Convention::Google) => {
            for context in &section_contexts(&lines, &SectionStyle::Google) {
                google_section(checker, docstring, context);
            }
        }
        Some(Convention::Numpy) => {
            for context in &section_contexts(&lines, &SectionStyle::Numpy) {
                numpy_section(checker, docstring, context);
            }
        }
        Some(Convention::Pep257) | None => {
            // First, interpret as NumPy-style sections.
            let mut found_numpy_section = false;
            for context in &section_contexts(&lines, &SectionStyle::Numpy) {
                found_numpy_section = true;
                numpy_section(checker, docstring, context);
            }

            // If no such sections were identified, interpret as Google-style sections.
            if !found_numpy_section {
                for context in &section_contexts(&lines, &SectionStyle::Google) {
                    google_section(checker, docstring, context);
                }
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
    for line in context.following_lines {
        if !line.trim().is_empty() {
            break;
        }
        blank_lines_after_header += 1;
    }

    // Nothing but blank lines after the section header.
    if blank_lines_after_header == context.following_lines.len() {
        if checker
            .settings
            .rules
            .enabled(&Rule::DashedUnderlineAfterSection)
        {
            let mut diagnostic = Diagnostic::new(
                DashedUnderlineAfterSection {
                    name: context.section_name.to_string(),
                },
                Range::from_located(docstring.expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Add a dashed line (of the appropriate length) under the section header.
                let content = format!(
                    "{}{}{}",
                    whitespace::clean(docstring.indentation),
                    "-".repeat(context.section_name.len()),
                    checker.stylist.line_ending().as_str()
                );
                diagnostic.amend(Fix::insertion(
                    content,
                    Location::new(
                        docstring.expr.location.row() + context.original_index + 1,
                        0,
                    ),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
        if checker.settings.rules.enabled(&Rule::NonEmptySection) {
            checker.diagnostics.push(Diagnostic::new(
                NonEmptySection {
                    name: context.section_name.to_string(),
                },
                Range::from_located(docstring.expr),
            ));
        }
        return;
    }

    let non_empty_line = context.following_lines[blank_lines_after_header];
    let dash_line_found = non_empty_line
        .chars()
        .all(|char| char.is_whitespace() || char == '-');

    if dash_line_found {
        if blank_lines_after_header > 0 {
            if checker
                .settings
                .rules
                .enabled(&Rule::SectionUnderlineAfterName)
            {
                let mut diagnostic = Diagnostic::new(
                    SectionUnderlineAfterName {
                        name: context.section_name.to_string(),
                    },
                    Range::from_located(docstring.expr),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // Delete any blank lines between the header and the underline.
                    diagnostic.amend(Fix::deletion(
                        Location::new(
                            docstring.expr.location.row() + context.original_index + 1,
                            0,
                        ),
                        Location::new(
                            docstring.expr.location.row()
                                + context.original_index
                                + 1
                                + blank_lines_after_header,
                            0,
                        ),
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }

        if non_empty_line
            .trim()
            .chars()
            .filter(|char| *char == '-')
            .count()
            != context.section_name.len()
        {
            if checker
                .settings
                .rules
                .enabled(&Rule::SectionUnderlineMatchesSectionLength)
            {
                let mut diagnostic = Diagnostic::new(
                    SectionUnderlineMatchesSectionLength {
                        name: context.section_name.to_string(),
                    },
                    Range::from_located(docstring.expr),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // Replace the existing underline with a line of the appropriate length.
                    let content = format!(
                        "{}{}{}",
                        whitespace::clean(docstring.indentation),
                        "-".repeat(context.section_name.len()),
                        checker.stylist.line_ending().as_str()
                    );
                    diagnostic.amend(Fix::replacement(
                        content,
                        Location::new(
                            docstring.expr.location.row()
                                + context.original_index
                                + 1
                                + blank_lines_after_header,
                            0,
                        ),
                        Location::new(
                            docstring.expr.location.row()
                                + context.original_index
                                + 1
                                + blank_lines_after_header
                                + 1,
                            0,
                        ),
                    ));
                };
                checker.diagnostics.push(diagnostic);
            }
        }

        if checker
            .settings
            .rules
            .enabled(&Rule::SectionUnderlineNotOverIndented)
        {
            let leading_space = whitespace::leading_space(non_empty_line);
            if leading_space.len() > docstring.indentation.len() {
                let mut diagnostic = Diagnostic::new(
                    SectionUnderlineNotOverIndented {
                        name: context.section_name.to_string(),
                    },
                    Range::from_located(docstring.expr),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // Replace the existing indentation with whitespace of the appropriate length.
                    diagnostic.amend(Fix::replacement(
                        whitespace::clean(docstring.indentation),
                        Location::new(
                            docstring.expr.location.row()
                                + context.original_index
                                + 1
                                + blank_lines_after_header,
                            0,
                        ),
                        Location::new(
                            docstring.expr.location.row()
                                + context.original_index
                                + 1
                                + blank_lines_after_header,
                            1 + leading_space.len(),
                        ),
                    ));
                };
                checker.diagnostics.push(diagnostic);
            }
        }

        let line_after_dashes_index = blank_lines_after_header + 1;

        if line_after_dashes_index < context.following_lines.len() {
            let line_after_dashes = context.following_lines[line_after_dashes_index];
            if line_after_dashes.trim().is_empty() {
                let rest_of_lines = &context.following_lines[line_after_dashes_index..];
                let blank_lines_after_dashes = rest_of_lines
                    .iter()
                    .take_while(|line| line.trim().is_empty())
                    .count();
                if blank_lines_after_dashes == rest_of_lines.len() {
                    if checker.settings.rules.enabled(&Rule::NonEmptySection) {
                        checker.diagnostics.push(Diagnostic::new(
                            NonEmptySection {
                                name: context.section_name.to_string(),
                            },
                            Range::from_located(docstring.expr),
                        ));
                    }
                } else {
                    if checker
                        .settings
                        .rules
                        .enabled(&Rule::NoBlankLinesBetweenHeaderAndContent)
                    {
                        let mut diagnostic = Diagnostic::new(
                            NoBlankLinesBetweenHeaderAndContent {
                                name: context.section_name.to_string(),
                            },
                            Range::from_located(docstring.expr),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            // Delete any blank lines between the header and content.
                            diagnostic.amend(Fix::deletion(
                                Location::new(
                                    docstring.expr.location.row()
                                        + context.original_index
                                        + 1
                                        + line_after_dashes_index,
                                    0,
                                ),
                                Location::new(
                                    docstring.expr.location.row()
                                        + context.original_index
                                        + 1
                                        + line_after_dashes_index
                                        + blank_lines_after_dashes,
                                    0,
                                ),
                            ));
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                }
            }
        } else {
            if checker.settings.rules.enabled(&Rule::NonEmptySection) {
                checker.diagnostics.push(Diagnostic::new(
                    NonEmptySection {
                        name: context.section_name.to_string(),
                    },
                    Range::from_located(docstring.expr),
                ));
            }
        }
    } else {
        if checker
            .settings
            .rules
            .enabled(&Rule::DashedUnderlineAfterSection)
        {
            let mut diagnostic = Diagnostic::new(
                DashedUnderlineAfterSection {
                    name: context.section_name.to_string(),
                },
                Range::from_located(docstring.expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Add a dashed line (of the appropriate length) under the section header.
                let content = format!(
                    "{}{}{}",
                    whitespace::clean(docstring.indentation),
                    "-".repeat(context.section_name.len()),
                    checker.stylist.line_ending().as_str()
                );
                diagnostic.amend(Fix::insertion(
                    content,
                    Location::new(
                        docstring.expr.location.row() + context.original_index + 1,
                        0,
                    ),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
        if blank_lines_after_header > 0 {
            if checker
                .settings
                .rules
                .enabled(&Rule::NoBlankLinesBetweenHeaderAndContent)
            {
                let mut diagnostic = Diagnostic::new(
                    NoBlankLinesBetweenHeaderAndContent {
                        name: context.section_name.to_string(),
                    },
                    Range::from_located(docstring.expr),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // Delete any blank lines between the header and content.
                    diagnostic.amend(Fix::deletion(
                        Location::new(
                            docstring.expr.location.row() + context.original_index + 1,
                            0,
                        ),
                        Location::new(
                            docstring.expr.location.row()
                                + context.original_index
                                + 1
                                + blank_lines_after_header,
                            0,
                        ),
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

fn common_section(
    checker: &mut Checker,
    docstring: &Docstring,
    context: &SectionContext,
    style: &SectionStyle,
) {
    if checker.settings.rules.enabled(&Rule::CapitalizeSectionName) {
        if !style.section_names().contains(&context.section_name) {
            let capitalized_section_name = titlecase::titlecase(context.section_name);
            if style
                .section_names()
                .contains(capitalized_section_name.as_str())
            {
                let mut diagnostic = Diagnostic::new(
                    CapitalizeSectionName {
                        name: context.section_name.to_string(),
                    },
                    Range::from_located(docstring.expr),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // Replace the section title with the capitalized variant. This requires
                    // locating the start and end of the section name.
                    if let Some(index) = context.line.find(context.section_name) {
                        // Map from bytes to characters.
                        let section_name_start = &context.line[..index].chars().count();
                        let section_name_length = &context.section_name.chars().count();
                        diagnostic.amend(Fix::replacement(
                            capitalized_section_name,
                            Location::new(
                                docstring.expr.location.row() + context.original_index,
                                *section_name_start,
                            ),
                            Location::new(
                                docstring.expr.location.row() + context.original_index,
                                section_name_start + section_name_length,
                            ),
                        ));
                    }
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }

    if checker
        .settings
        .rules
        .enabled(&Rule::SectionNotOverIndented)
    {
        let leading_space = whitespace::leading_space(context.line);
        if leading_space.len() > docstring.indentation.len() {
            let mut diagnostic = Diagnostic::new(
                SectionNotOverIndented {
                    name: context.section_name.to_string(),
                },
                Range::from_located(docstring.expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Replace the existing indentation with whitespace of the appropriate length.
                diagnostic.amend(Fix::replacement(
                    whitespace::clean(docstring.indentation),
                    Location::new(docstring.expr.location.row() + context.original_index, 0),
                    Location::new(
                        docstring.expr.location.row() + context.original_index,
                        leading_space.len(),
                    ),
                ));
            };
            checker.diagnostics.push(diagnostic);
        }
    }

    let line_end = checker.stylist.line_ending().as_str();
    if context
        .following_lines
        .last()
        .map_or(true, |line| !line.trim().is_empty())
    {
        if context.is_last_section {
            if checker
                .settings
                .rules
                .enabled(&Rule::BlankLineAfterLastSection)
            {
                let mut diagnostic = Diagnostic::new(
                    BlankLineAfterLastSection {
                        name: context.section_name.to_string(),
                    },
                    Range::from_located(docstring.expr),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // Add a newline after the section.
                    diagnostic.amend(Fix::insertion(
                        line_end.to_string(),
                        Location::new(
                            docstring.expr.location.row()
                                + context.original_index
                                + 1
                                + context.following_lines.len(),
                            0,
                        ),
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        } else {
            if checker.settings.rules.enabled(&Rule::BlankLineAfterSection) {
                let mut diagnostic = Diagnostic::new(
                    BlankLineAfterSection {
                        name: context.section_name.to_string(),
                    },
                    Range::from_located(docstring.expr),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // Add a newline after the section.
                    diagnostic.amend(Fix::insertion(
                        line_end.to_string(),
                        Location::new(
                            docstring.expr.location.row()
                                + context.original_index
                                + 1
                                + context.following_lines.len(),
                            0,
                        ),
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }

    if checker
        .settings
        .rules
        .enabled(&Rule::BlankLineBeforeSection)
    {
        if !context.previous_line.is_empty() {
            let mut diagnostic = Diagnostic::new(
                BlankLineBeforeSection {
                    name: context.section_name.to_string(),
                },
                Range::from_located(docstring.expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Add a blank line before the section.
                diagnostic.amend(Fix::insertion(
                    line_end.to_string(),
                    Location::new(docstring.expr.location.row() + context.original_index, 0),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }

    blanks_and_section_underline(checker, docstring, context);
}

fn missing_args(checker: &mut Checker, docstring: &Docstring, docstrings_args: &FxHashSet<&str>) {
    let (
        DefinitionKind::Function(parent)
        | DefinitionKind::NestedFunction(parent)
        | DefinitionKind::Method(parent)
    ) = docstring.kind else {
        return
    };
    let (
        StmtKind::FunctionDef {
            args: arguments, ..
        }
        | StmtKind::AsyncFunctionDef {
            args: arguments, ..
        }
    ) = &parent.node else {
        return
    };

    // Look for arguments that weren't included in the docstring.
    let mut missing_arg_names: FxHashSet<String> = FxHashSet::default();
    for arg in arguments
        .args
        .iter()
        .chain(arguments.posonlyargs.iter())
        .chain(arguments.kwonlyargs.iter())
        .skip(
            // If this is a non-static method, skip `cls` or `self`.
            usize::from(
                matches!(docstring.kind, DefinitionKind::Method(_))
                    && !is_staticmethod(checker, cast::decorator_list(parent)),
            ),
        )
    {
        let arg_name = arg.node.arg.as_str();
        if !arg_name.starts_with('_') && !docstrings_args.contains(&arg_name) {
            missing_arg_names.insert(arg_name.to_string());
        }
    }

    // Check specifically for `vararg` and `kwarg`, which can be prefixed with a
    // single or double star, respectively.
    if let Some(arg) = &arguments.vararg {
        let arg_name = arg.node.arg.as_str();
        let starred_arg_name = format!("*{arg_name}");
        if !arg_name.starts_with('_')
            && !docstrings_args.contains(&arg_name)
            && !docstrings_args.contains(&starred_arg_name.as_str())
        {
            missing_arg_names.insert(starred_arg_name);
        }
    }
    if let Some(arg) = &arguments.kwarg {
        let arg_name = arg.node.arg.as_str();
        let starred_arg_name = format!("**{arg_name}");
        if !arg_name.starts_with('_')
            && !docstrings_args.contains(&arg_name)
            && !docstrings_args.contains(&starred_arg_name.as_str())
        {
            missing_arg_names.insert(starred_arg_name);
        }
    }

    if !missing_arg_names.is_empty() {
        let names = missing_arg_names.into_iter().sorted().collect();
        checker.diagnostics.push(Diagnostic::new(
            DocumentAllArguments { names },
            Range::from_located(parent),
        ));
    }
}

// See: `GOOGLE_ARGS_REGEX` in `pydocstyle/checker.py`.
static GOOGLE_ARGS_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(\*?\*?\w+)\s*(\(.*?\))?\s*:(\r\n|\n)?\s*.+").unwrap());

fn args_section(checker: &mut Checker, docstring: &Docstring, context: &SectionContext) {
    if context.following_lines.is_empty() {
        missing_args(checker, docstring, &FxHashSet::default());
        return;
    }

    // Normalize leading whitespace, by removing any lines with less indentation
    // than the first.
    let leading_space = whitespace::leading_space(context.following_lines[0]);
    let relevant_lines = context
        .following_lines
        .iter()
        .filter(|line| line.starts_with(leading_space) || line.is_empty())
        .join("\n");
    let args_content = textwrap::dedent(&relevant_lines);

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
    let docstrings_args = matches
        .iter()
        .filter_map(|captures| captures.get(1).map(|arg_name| arg_name.as_str()))
        .collect();

    missing_args(checker, docstring, &docstrings_args);
}

fn parameters_section(checker: &mut Checker, docstring: &Docstring, context: &SectionContext) {
    // Collect the list of arguments documented in the docstring.
    let mut docstring_args: FxHashSet<&str> = FxHashSet::default();
    let section_level_indent = whitespace::leading_space(context.line);

    // Join line continuations, then resplit by line.
    let adjusted_following_lines = context.following_lines.join("\n").replace("\\\n", "");
    let lines: Vec<&str> = LinesWithTrailingNewline::from(&adjusted_following_lines).collect();

    for i in 1..lines.len() {
        let current_line = lines[i - 1];
        let current_leading_space = whitespace::leading_space(current_line);
        let next_line = lines[i];
        if current_leading_space == section_level_indent
            && (whitespace::leading_space(next_line).len() > current_leading_space.len())
            && !next_line.trim().is_empty()
        {
            let parameters = if let Some(semi_index) = current_line.find(':') {
                // If the parameter has a type annotation, exclude it.
                &current_line[..semi_index]
            } else {
                // Otherwise, it's just a list of parameters on the current line.
                current_line.trim()
            };
            // Notably, NumPy lets you put multiple parameters of the same type on the same
            // line.
            for parameter in parameters.split(',') {
                docstring_args.insert(parameter.trim());
            }
        }
    }
    // Validate that all arguments were documented.
    missing_args(checker, docstring, &docstring_args);
}

fn numpy_section(checker: &mut Checker, docstring: &Docstring, context: &SectionContext) {
    common_section(checker, docstring, context, &SectionStyle::Numpy);

    if checker
        .settings
        .rules
        .enabled(&Rule::NewLineAfterSectionName)
    {
        let suffix = context
            .line
            .trim()
            .strip_prefix(context.section_name)
            .unwrap();
        if !suffix.is_empty() {
            let mut diagnostic = Diagnostic::new(
                NewLineAfterSectionName {
                    name: context.section_name.to_string(),
                },
                Range::from_located(docstring.expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Delete the suffix. This requires locating the end of the section name.
                if let Some(index) = context.line.find(context.section_name) {
                    // Map from bytes to characters.
                    let suffix_start = &context.line[..index + context.section_name.len()]
                        .chars()
                        .count();
                    let suffix_length = suffix.chars().count();
                    diagnostic.amend(Fix::deletion(
                        Location::new(
                            docstring.expr.location.row() + context.original_index,
                            *suffix_start,
                        ),
                        Location::new(
                            docstring.expr.location.row() + context.original_index,
                            suffix_start + suffix_length,
                        ),
                    ));
                }
            }
            checker.diagnostics.push(diagnostic);
        }
    }

    if checker.settings.rules.enabled(&Rule::DocumentAllArguments) {
        let capitalized_section_name = titlecase::titlecase(context.section_name);
        if capitalized_section_name == "Parameters" {
            parameters_section(checker, docstring, context);
        }
    }
}

fn google_section(checker: &mut Checker, docstring: &Docstring, context: &SectionContext) {
    common_section(checker, docstring, context, &SectionStyle::Google);

    if checker
        .settings
        .rules
        .enabled(&Rule::SectionNameEndsInColon)
    {
        let suffix = context
            .line
            .trim()
            .strip_prefix(context.section_name)
            .unwrap();
        if suffix != ":" {
            let mut diagnostic = Diagnostic::new(
                SectionNameEndsInColon {
                    name: context.section_name.to_string(),
                },
                Range::from_located(docstring.expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Replace the suffix. This requires locating the end of the section name.
                if let Some(index) = context.line.find(context.section_name) {
                    // Map from bytes to characters.
                    let suffix_start = &context.line[..index + context.section_name.len()]
                        .chars()
                        .count();
                    let suffix_length = suffix.chars().count();
                    diagnostic.amend(Fix::replacement(
                        ":".to_string(),
                        Location::new(
                            docstring.expr.location.row() + context.original_index,
                            *suffix_start,
                        ),
                        Location::new(
                            docstring.expr.location.row() + context.original_index,
                            suffix_start + suffix_length,
                        ),
                    ));
                }
            }
            checker.diagnostics.push(diagnostic);
        }
    }

    if checker.settings.rules.enabled(&Rule::DocumentAllArguments) {
        let capitalized_section_name = titlecase::titlecase(context.section_name);
        if capitalized_section_name == "Args" || capitalized_section_name == "Arguments" {
            args_section(checker, docstring, context);
        }
    }
}
