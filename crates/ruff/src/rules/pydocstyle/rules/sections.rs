use itertools::Itertools;
use once_cell::sync::Lazy;
use regex::Regex;
use ruff_text_size::{TextLen, TextRange, TextSize};
use rustc_hash::FxHashSet;
use rustpython_parser::ast::StmtKind;

use ruff_diagnostics::{AlwaysAutofixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::identifier_range;
use ruff_python_ast::newlines::NewlineWithTrailingNewline;
use ruff_python_ast::{cast, whitespace};
use ruff_python_semantic::analyze::visibility::is_staticmethod;

use crate::checkers::ast::Checker;
use crate::docstrings::definition::{DefinitionKind, Docstring};
use crate::docstrings::sections::{SectionContext, SectionContexts, SectionKind};
use crate::docstrings::styles::SectionStyle;
use crate::registry::{AsRule, Rule};
use crate::rules::pydocstyle::settings::Convention;

#[violation]
pub struct SectionNotOverIndented {
    pub name: String,
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

#[violation]
pub struct SectionUnderlineNotOverIndented {
    pub name: String,
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

#[violation]
pub struct CapitalizeSectionName {
    pub name: String,
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

#[violation]
pub struct NewLineAfterSectionName {
    pub name: String,
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

#[violation]
pub struct DashedUnderlineAfterSection {
    pub name: String,
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

#[violation]
pub struct SectionUnderlineAfterName {
    pub name: String,
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

#[violation]
pub struct SectionUnderlineMatchesSectionLength {
    pub name: String,
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

#[violation]
pub struct NoBlankLineAfterSection {
    pub name: String,
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

#[violation]
pub struct NoBlankLineBeforeSection {
    pub name: String,
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

#[violation]
pub struct BlankLineAfterLastSection {
    pub name: String,
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

#[violation]
pub struct EmptyDocstringSection {
    pub name: String,
}

impl Violation for EmptyDocstringSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let EmptyDocstringSection { name } = self;
        format!("Section has no content (\"{name}\")")
    }
}

#[violation]
pub struct SectionNameEndsInColon {
    pub name: String,
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

#[violation]
pub struct UndocumentedParam {
    pub names: Vec<String>,
}

impl Violation for UndocumentedParam {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndocumentedParam { names } = self;
        if names.len() == 1 {
            let name = &names[0];
            format!("Missing argument description in the docstring: `{name}`")
        } else {
            let names = names.iter().map(|name| format!("`{name}`")).join(", ");
            format!("Missing argument descriptions in the docstring: {names}")
        }
    }
}

#[violation]
pub struct BlankLinesBetweenHeaderAndContent {
    pub name: String,
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
pub fn sections(checker: &mut Checker, docstring: &Docstring, convention: Option<&Convention>) {
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
        let dash_line_found = non_blank_line
            .chars()
            .all(|char| char.is_whitespace() || char == '-');

        if dash_line_found {
            if blank_lines_after_header > 0 {
                if checker
                    .settings
                    .rules
                    .enabled(Rule::SectionUnderlineAfterName)
                {
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
                        diagnostic.set_fix(Edit::range_deletion(range));
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
                if checker
                    .settings
                    .rules
                    .enabled(Rule::SectionUnderlineMatchesSectionLength)
                {
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
                            whitespace::clean(docstring.indentation),
                            "-".repeat(context.section_name().len()),
                            checker.stylist.line_ending().as_str()
                        );
                        diagnostic.set_fix(Edit::replacement(
                            content,
                            blank_lines_end,
                            non_blank_line.full_end(),
                        ));
                    };
                    checker.diagnostics.push(diagnostic);
                }
            }

            if checker
                .settings
                .rules
                .enabled(Rule::SectionUnderlineNotOverIndented)
            {
                let leading_space = whitespace::leading_space(&non_blank_line);
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
                        diagnostic.set_fix(Edit::range_replacement(
                            whitespace::clean(docstring.indentation),
                            range,
                        ));
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
                        if checker.settings.rules.enabled(Rule::EmptyDocstringSection) {
                            checker.diagnostics.push(Diagnostic::new(
                                EmptyDocstringSection {
                                    name: context.section_name().to_string(),
                                },
                                docstring.range(),
                            ));
                        }
                    } else if checker
                        .settings
                        .rules
                        .enabled(Rule::BlankLinesBetweenHeaderAndContent)
                    {
                        let mut diagnostic = Diagnostic::new(
                            BlankLinesBetweenHeaderAndContent {
                                name: context.section_name().to_string(),
                            },
                            docstring.range(),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            // Delete any blank lines between the header and content.
                            diagnostic.set_fix(Edit::deletion(
                                line_after_dashes.start(),
                                blank_lines_after_dashes_end,
                            ));
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                }
            } else {
                if checker.settings.rules.enabled(Rule::EmptyDocstringSection) {
                    checker.diagnostics.push(Diagnostic::new(
                        EmptyDocstringSection {
                            name: context.section_name().to_string(),
                        },
                        docstring.range(),
                    ));
                }
            }
        } else {
            if checker
                .settings
                .rules
                .enabled(Rule::DashedUnderlineAfterSection)
            {
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
                        checker.stylist.line_ending().as_str(),
                        whitespace::clean(docstring.indentation),
                        "-".repeat(context.section_name().len()),
                    );
                    diagnostic.set_fix(Edit::insertion(content, context.summary_range().end()));
                }
                checker.diagnostics.push(diagnostic);
            }
            if blank_lines_after_header > 0 {
                if checker
                    .settings
                    .rules
                    .enabled(Rule::BlankLinesBetweenHeaderAndContent)
                {
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
                        diagnostic.set_fix(Edit::range_deletion(range));
                    }
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
    }
    // Nothing but blank lines after the section header.
    else {
        if checker
            .settings
            .rules
            .enabled(Rule::DashedUnderlineAfterSection)
        {
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
                    checker.stylist.line_ending().as_str(),
                    whitespace::clean(docstring.indentation),
                    "-".repeat(context.section_name().len()),
                );

                diagnostic.set_fix(Edit::insertion(content, context.summary_range().end()));
            }
            checker.diagnostics.push(diagnostic);
        }
        if checker.settings.rules.enabled(Rule::EmptyDocstringSection) {
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
    if checker.settings.rules.enabled(Rule::CapitalizeSectionName) {
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
                diagnostic.set_fix(Edit::range_replacement(
                    capitalized_section_name.to_string(),
                    section_range,
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }

    if checker.settings.rules.enabled(Rule::SectionNotOverIndented) {
        let leading_space = whitespace::leading_space(context.summary_line());
        if leading_space.len() > docstring.indentation.len() {
            let mut diagnostic = Diagnostic::new(
                SectionNotOverIndented {
                    name: context.section_name().to_string(),
                },
                docstring.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Replace the existing indentation with whitespace of the appropriate length.
                let content = whitespace::clean(docstring.indentation);
                let fix_range = TextRange::at(context.range().start(), leading_space.text_len());

                diagnostic.set_fix(if content.is_empty() {
                    Edit::range_deletion(fix_range)
                } else {
                    Edit::range_replacement(content, fix_range)
                });
            };
            checker.diagnostics.push(diagnostic);
        }
    }

    let line_end = checker.stylist.line_ending().as_str();
    let last_line = context.following_lines().last();
    if last_line.map_or(true, |line| !line.trim().is_empty()) {
        if let Some(next) = next {
            if checker
                .settings
                .rules
                .enabled(Rule::NoBlankLineAfterSection)
            {
                let mut diagnostic = Diagnostic::new(
                    NoBlankLineAfterSection {
                        name: context.section_name().to_string(),
                    },
                    docstring.range(),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // Add a newline at the beginning of the next section.
                    diagnostic.set_fix(Edit::insertion(line_end.to_string(), next.range().start()));
                }
                checker.diagnostics.push(diagnostic);
            }
        } else {
            if checker
                .settings
                .rules
                .enabled(Rule::BlankLineAfterLastSection)
            {
                let mut diagnostic = Diagnostic::new(
                    BlankLineAfterLastSection {
                        name: context.section_name().to_string(),
                    },
                    docstring.range(),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // Add a newline after the section.
                    diagnostic.set_fix(Edit::insertion(
                        format!("{}{}", line_end, docstring.indentation),
                        context.range().end(),
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }

    if checker
        .settings
        .rules
        .enabled(Rule::NoBlankLineBeforeSection)
    {
        if !context.previous_line().map_or(false, str::is_empty) {
            let mut diagnostic = Diagnostic::new(
                NoBlankLineBeforeSection {
                    name: context.section_name().to_string(),
                },
                docstring.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Add a blank line before the section.
                diagnostic.set_fix(Edit::insertion(
                    line_end.to_string(),
                    context.range().start(),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }

    blanks_and_section_underline(checker, docstring, context);
}

fn missing_args(checker: &mut Checker, docstring: &Docstring, docstrings_args: &FxHashSet<String>) {
    let (
        DefinitionKind::Function(parent)
        | DefinitionKind::NestedFunction(parent)
        | DefinitionKind::Method(parent)
    ) = docstring.kind else {
        return;
    };
    let (
        StmtKind::FunctionDef {
            args: arguments, ..
        }
        | StmtKind::AsyncFunctionDef {
            args: arguments, ..
        }
    ) = &parent.node else {
        return;
    };

    // Look for arguments that weren't included in the docstring.
    let mut missing_arg_names: FxHashSet<String> = FxHashSet::default();
    for arg in arguments
        .posonlyargs
        .iter()
        .chain(arguments.args.iter())
        .chain(arguments.kwonlyargs.iter())
        .skip(
            // If this is a non-static method, skip `cls` or `self`.
            usize::from(
                matches!(docstring.kind, DefinitionKind::Method(_))
                    && !is_staticmethod(&checker.ctx, cast::decorator_list(parent)),
            ),
        )
    {
        let arg_name = &arg.node.arg;
        if !arg_name.starts_with('_') && !docstrings_args.contains(arg_name) {
            missing_arg_names.insert(arg_name.to_string());
        }
    }

    // Check specifically for `vararg` and `kwarg`, which can be prefixed with a
    // single or double star, respectively.
    if let Some(arg) = &arguments.vararg {
        let arg_name = &arg.node.arg;
        let starred_arg_name = format!("*{arg_name}");
        if !arg_name.starts_with('_')
            && !docstrings_args.contains(arg_name)
            && !docstrings_args.contains(&starred_arg_name)
        {
            missing_arg_names.insert(starred_arg_name);
        }
    }
    if let Some(arg) = &arguments.kwarg {
        let arg_name = &arg.node.arg;
        let starred_arg_name = format!("**{arg_name}");
        if !arg_name.starts_with('_')
            && !docstrings_args.contains(arg_name)
            && !docstrings_args.contains(&starred_arg_name)
        {
            missing_arg_names.insert(starred_arg_name);
        }
    }

    if !missing_arg_names.is_empty() {
        let names = missing_arg_names.into_iter().sorted().collect();
        checker.diagnostics.push(Diagnostic::new(
            UndocumentedParam { names },
            identifier_range(parent, checker.locator),
        ));
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
    let leading_space = whitespace::leading_space(first_line.as_str());
    let relevant_lines = std::iter::once(first_line)
        .chain(following_lines)
        .map(|l| l.as_str())
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

    matches
        .iter()
        .filter_map(|captures| captures.get(1).map(|arg_name| arg_name.as_str().to_owned()))
        .collect::<FxHashSet<String>>()
}

fn parameters_section(checker: &mut Checker, docstring: &Docstring, context: &SectionContext) {
    // Collect the list of arguments documented in the docstring.
    let mut docstring_args: FxHashSet<String> = FxHashSet::default();
    let section_level_indent = whitespace::leading_space(context.summary_line());

    // Join line continuations, then resplit by line.
    let adjusted_following_lines = context
        .following_lines()
        .map(|l| l.as_str())
        .join("\n")
        .replace("\\\n", "");
    let mut lines = NewlineWithTrailingNewline::from(&adjusted_following_lines);
    if let Some(mut current_line) = lines.next() {
        for next_line in lines {
            let current_leading_space = whitespace::leading_space(current_line.as_str());
            if current_leading_space == section_level_indent
                && (whitespace::leading_space(&next_line).len() > current_leading_space.len())
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

    if checker
        .settings
        .rules
        .enabled(Rule::NewLineAfterSectionName)
    {
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
                diagnostic.set_fix(Edit::range_deletion(TextRange::at(
                    section_range.end(),
                    suffix.text_len(),
                )));
            }

            checker.diagnostics.push(diagnostic);
        }
    }

    if checker.settings.rules.enabled(Rule::UndocumentedParam) {
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

    if checker.settings.rules.enabled(Rule::SectionNameEndsInColon) {
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
                diagnostic.set_fix(Edit::range_replacement(
                    ":".to_string(),
                    TextRange::at(section_name_range.end(), suffix.text_len()),
                ));
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

    if checker.settings.rules.enabled(Rule::UndocumentedParam) {
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
