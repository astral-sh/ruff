use std::collections::BTreeSet;

use fnv::FnvHashSet;
use itertools::Itertools;
use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_ast::{Arg, Constant, ExprKind, Location, StmtKind};

use crate::ast::types::Range;
use crate::ast::whitespace;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckCode, CheckKind};
use crate::docstrings::constants;
use crate::docstrings::definition::{Definition, DefinitionKind};
use crate::docstrings::sections::{section_contexts, SectionContext};
use crate::docstrings::styles::SectionStyle;
use crate::visibility::{is_init, is_magic, is_overload, is_staticmethod, Visibility};

/// D100, D101, D102, D103, D104, D105, D106, D107
pub fn not_missing(
    checker: &mut Checker,
    definition: &Definition,
    visibility: &Visibility,
) -> bool {
    if matches!(visibility, Visibility::Private) {
        return true;
    }

    if definition.docstring.is_some() {
        return true;
    }

    match definition.kind {
        DefinitionKind::Module => {
            if checker.settings.enabled.contains(&CheckCode::D100) {
                checker.add_check(Check::new(
                    CheckKind::PublicModule,
                    Range {
                        location: Location::new(1, 0),
                        end_location: Location::new(1, 0),
                    },
                ));
            }
            false
        }
        DefinitionKind::Package => {
            if checker.settings.enabled.contains(&CheckCode::D104) {
                checker.add_check(Check::new(
                    CheckKind::PublicPackage,
                    Range {
                        location: Location::new(1, 0),
                        end_location: Location::new(1, 0),
                    },
                ));
            }
            false
        }
        DefinitionKind::Class(stmt) => {
            if checker.settings.enabled.contains(&CheckCode::D101) {
                checker.add_check(Check::new(
                    CheckKind::PublicClass,
                    Range::from_located(stmt),
                ));
            }
            false
        }
        DefinitionKind::NestedClass(stmt) => {
            if checker.settings.enabled.contains(&CheckCode::D106) {
                checker.add_check(Check::new(
                    CheckKind::PublicNestedClass,
                    Range::from_located(stmt),
                ));
            }
            false
        }
        DefinitionKind::Function(stmt) | DefinitionKind::NestedFunction(stmt) => {
            if is_overload(stmt) {
                true
            } else {
                if checker.settings.enabled.contains(&CheckCode::D103) {
                    checker.add_check(Check::new(
                        CheckKind::PublicFunction,
                        Range::from_located(stmt),
                    ));
                }
                false
            }
        }
        DefinitionKind::Method(stmt) => {
            if is_overload(stmt) {
                true
            } else if is_magic(stmt) {
                if checker.settings.enabled.contains(&CheckCode::D105) {
                    checker.add_check(Check::new(
                        CheckKind::MagicMethod,
                        Range::from_located(stmt),
                    ));
                }
                true
            } else if is_init(stmt) {
                if checker.settings.enabled.contains(&CheckCode::D107) {
                    checker.add_check(Check::new(CheckKind::PublicInit, Range::from_located(stmt)));
                }
                true
            } else {
                if checker.settings.enabled.contains(&CheckCode::D102) {
                    checker.add_check(Check::new(
                        CheckKind::PublicMethod,
                        Range::from_located(stmt),
                    ));
                }
                true
            }
        }
    }
}

/// D200
pub fn one_liner(checker: &mut Checker, definition: &Definition) {
    if let Some(docstring) = &definition.docstring {
        if let ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } = &docstring.node
        {
            let mut line_count = 0;
            let mut non_empty_line_count = 0;
            for line in string.lines() {
                line_count += 1;
                if !line.trim().is_empty() {
                    non_empty_line_count += 1;
                }
                if non_empty_line_count > 1 {
                    break;
                }
            }

            if non_empty_line_count == 1 && line_count > 1 {
                checker.add_check(Check::new(
                    CheckKind::FitsOnOneLine,
                    Range::from_located(docstring),
                ));
            }
        }
    }
}

static COMMENT_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*#").unwrap());

static INNER_FUNCTION_OR_CLASS_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s+(?:(?:class|def|async def)\s|@)").unwrap());

/// D201, D202
pub fn blank_before_after_function(checker: &mut Checker, definition: &Definition) {
    if let Some(docstring) = definition.docstring {
        if let DefinitionKind::Function(parent)
        | DefinitionKind::NestedFunction(parent)
        | DefinitionKind::Method(parent) = &definition.kind
        {
            if let ExprKind::Constant {
                value: Constant::Str(_),
                ..
            } = &docstring.node
            {
                if checker.settings.enabled.contains(&CheckCode::D201) {
                    let (before, ..) = checker.locator.partition_source_code_at(
                        &Range::from_located(parent),
                        &Range::from_located(docstring),
                    );

                    let blank_lines_before = before
                        .lines()
                        .rev()
                        .skip(1)
                        .take_while(|line| line.trim().is_empty())
                        .count();
                    if blank_lines_before != 0 {
                        let mut check = Check::new(
                            CheckKind::NoBlankLineBeforeFunction(blank_lines_before),
                            Range::from_located(docstring),
                        );
                        if checker.patch(check.kind.code()) {
                            // Delete the blank line before the docstring.
                            check.amend(Fix::deletion(
                                Location::new(docstring.location.row() - blank_lines_before, 0),
                                Location::new(docstring.location.row(), 0),
                            ));
                        }
                        checker.add_check(check);
                    }
                }

                if checker.settings.enabled.contains(&CheckCode::D202) {
                    let (_, _, after) = checker.locator.partition_source_code_at(
                        &Range::from_located(parent),
                        &Range::from_located(docstring),
                    );

                    let all_blank_after = after
                        .lines()
                        .skip(1)
                        .all(|line| line.trim().is_empty() || COMMENT_REGEX.is_match(line));
                    if all_blank_after {
                        return;
                    }

                    let blank_lines_after = after
                        .lines()
                        .skip(1)
                        .take_while(|line| line.trim().is_empty())
                        .count();

                    // Avoid D202 violations for blank lines followed by inner functions or classes.
                    if blank_lines_after == 1 && INNER_FUNCTION_OR_CLASS_REGEX.is_match(&after) {
                        return;
                    }

                    if blank_lines_after != 0 {
                        let mut check = Check::new(
                            CheckKind::NoBlankLineAfterFunction(blank_lines_after),
                            Range::from_located(docstring),
                        );
                        if checker.patch(check.kind.code()) {
                            // Delete the blank line after the docstring.
                            check.amend(Fix::deletion(
                                Location::new(docstring.end_location.unwrap().row() + 1, 0),
                                Location::new(
                                    docstring.end_location.unwrap().row() + 1 + blank_lines_after,
                                    0,
                                ),
                            ));
                        }
                        checker.add_check(check);
                    }
                }
            }
        }
    }
}

/// D203, D204, D211
pub fn blank_before_after_class(checker: &mut Checker, definition: &Definition) {
    if let Some(docstring) = &definition.docstring {
        if let DefinitionKind::Class(parent) | DefinitionKind::NestedClass(parent) =
            &definition.kind
        {
            if let ExprKind::Constant {
                value: Constant::Str(_),
                ..
            } = &docstring.node
            {
                if checker.settings.enabled.contains(&CheckCode::D203)
                    || checker.settings.enabled.contains(&CheckCode::D211)
                {
                    let (before, ..) = checker.locator.partition_source_code_at(
                        &Range::from_located(parent),
                        &Range::from_located(docstring),
                    );

                    let blank_lines_before = before
                        .lines()
                        .rev()
                        .skip(1)
                        .take_while(|line| line.trim().is_empty())
                        .count();
                    if checker.settings.enabled.contains(&CheckCode::D211) {
                        if blank_lines_before != 0 {
                            let mut check = Check::new(
                                CheckKind::NoBlankLineBeforeClass(blank_lines_before),
                                Range::from_located(docstring),
                            );
                            if checker.patch(check.kind.code()) {
                                // Delete the blank line before the class.
                                check.amend(Fix::deletion(
                                    Location::new(docstring.location.row() - blank_lines_before, 0),
                                    Location::new(docstring.location.row(), 0),
                                ));
                            }
                            checker.add_check(check);
                        }
                    }
                    if checker.settings.enabled.contains(&CheckCode::D203) {
                        if blank_lines_before != 1 {
                            let mut check = Check::new(
                                CheckKind::OneBlankLineBeforeClass(blank_lines_before),
                                Range::from_located(docstring),
                            );
                            if checker.patch(check.kind.code()) {
                                // Insert one blank line before the class.
                                check.amend(Fix::replacement(
                                    "\n".to_string(),
                                    Location::new(docstring.location.row() - blank_lines_before, 0),
                                    Location::new(docstring.location.row(), 0),
                                ));
                            }
                            checker.add_check(check);
                        }
                    }
                }

                if checker.settings.enabled.contains(&CheckCode::D204) {
                    let (_, _, after) = checker.locator.partition_source_code_at(
                        &Range::from_located(parent),
                        &Range::from_located(docstring),
                    );

                    let all_blank_after = after
                        .lines()
                        .skip(1)
                        .all(|line| line.trim().is_empty() || COMMENT_REGEX.is_match(line));
                    if all_blank_after {
                        return;
                    }

                    let blank_lines_after = after
                        .lines()
                        .skip(1)
                        .take_while(|line| line.trim().is_empty())
                        .count();
                    if blank_lines_after != 1 {
                        let mut check = Check::new(
                            CheckKind::OneBlankLineAfterClass(blank_lines_after),
                            Range::from_located(docstring),
                        );
                        if checker.patch(check.kind.code()) {
                            // Insert a blank line before the class (replacing any existing lines).
                            check.amend(Fix::replacement(
                                "\n".to_string(),
                                Location::new(docstring.end_location.unwrap().row() + 1, 0),
                                Location::new(
                                    docstring.end_location.unwrap().row() + 1 + blank_lines_after,
                                    0,
                                ),
                            ));
                        }
                        checker.add_check(check);
                    }
                }
            }
        }
    }
}

/// D205
pub fn blank_after_summary(checker: &mut Checker, definition: &Definition) {
    if let Some(docstring) = definition.docstring {
        if let ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } = &docstring.node
        {
            let mut lines_count = 1;
            let mut blanks_count = 0;
            for line in string.trim().lines().skip(1) {
                lines_count += 1;
                if line.trim().is_empty() {
                    blanks_count += 1;
                } else {
                    break;
                }
            }
            if lines_count > 1 && blanks_count != 1 {
                let mut check = Check::new(
                    CheckKind::BlankLineAfterSummary,
                    Range::from_located(docstring),
                );
                if checker.patch(check.kind.code()) {
                    // Insert one blank line after the summary (replacing any existing lines).
                    check.amend(Fix::replacement(
                        "\n".to_string(),
                        Location::new(docstring.location.row() + 1, 0),
                        Location::new(docstring.location.row() + 1 + blanks_count, 0),
                    ));
                }
                checker.add_check(check);
            }
        }
    }
}

/// D206, D207, D208
pub fn indent(checker: &mut Checker, definition: &Definition) {
    if let Some(docstring) = definition.docstring {
        if let ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } = &docstring.node
        {
            let lines: Vec<&str> = string.lines().collect();
            if lines.len() <= 1 {
                return;
            }

            let docstring_indent = whitespace::indentation(checker, docstring);
            let mut has_seen_tab = docstring_indent.contains('\t');
            let mut is_over_indented = true;
            let mut over_indented_lines = vec![];
            for i in 0..lines.len() {
                // First lines and continuations doesn't need any indentation.
                if i == 0 || lines[i - 1].ends_with('\\') {
                    continue;
                }

                // Omit empty lines, except for the last line, which is non-empty by way of
                // containing the closing quotation marks.
                let is_blank = lines[i].trim().is_empty();
                if i < lines.len() - 1 && is_blank {
                    continue;
                }

                let line_indent = whitespace::leading_space(lines[i]);

                // We only report tab indentation once, so only check if we haven't seen a tab
                // yet.
                has_seen_tab = has_seen_tab || line_indent.contains('\t');

                if checker.settings.enabled.contains(&CheckCode::D207) {
                    // We report under-indentation on every line. This isn't great, but enables
                    // autofix.
                    if !is_blank && line_indent.len() < docstring_indent.len() {
                        let mut check = Check::new(
                            CheckKind::NoUnderIndentation,
                            Range {
                                location: Location::new(docstring.location.row() + i, 0),
                                end_location: Location::new(docstring.location.row() + i, 0),
                            },
                        );
                        if checker.patch(check.kind.code()) {
                            check.amend(Fix::replacement(
                                whitespace::clean(&docstring_indent),
                                Location::new(docstring.location.row() + i, 0),
                                Location::new(docstring.location.row() + i, line_indent.len()),
                            ));
                        }
                        checker.add_check(check);
                    }
                }

                // Like pydocstyle, we only report over-indentation if either: (1) every line
                // (except, optionally, the last line) is over-indented, or (2) the last line
                // (which contains the closing quotation marks) is
                // over-indented. We can't know if we've achieved that condition
                // until we've viewed all the lines, so for now, just track
                // the over-indentation status of every line.
                if i < lines.len() - 1 {
                    if line_indent.len() > docstring_indent.len() {
                        over_indented_lines.push(i);
                    } else {
                        is_over_indented = false;
                    }
                }
            }

            if checker.settings.enabled.contains(&CheckCode::D206) {
                if has_seen_tab {
                    checker.add_check(Check::new(
                        CheckKind::IndentWithSpaces,
                        Range::from_located(docstring),
                    ));
                }
            }

            if checker.settings.enabled.contains(&CheckCode::D208) {
                // If every line (except the last) is over-indented...
                if is_over_indented {
                    for i in over_indented_lines {
                        let line_indent = whitespace::leading_space(lines[i]);
                        if line_indent.len() > docstring_indent.len() {
                            // We report over-indentation on every line. This isn't great, but
                            // enables autofix.
                            let mut check = Check::new(
                                CheckKind::NoOverIndentation,
                                Range {
                                    location: Location::new(docstring.location.row() + i, 0),
                                    end_location: Location::new(docstring.location.row() + i, 0),
                                },
                            );
                            if checker.patch(check.kind.code()) {
                                check.amend(Fix::replacement(
                                    whitespace::clean(&docstring_indent),
                                    Location::new(docstring.location.row() + i, 0),
                                    Location::new(docstring.location.row() + i, line_indent.len()),
                                ));
                            }
                            checker.add_check(check);
                        }
                    }
                }

                // If the last line is over-indented...
                if !lines.is_empty() {
                    let i = lines.len() - 1;
                    let line_indent = whitespace::leading_space(lines[i]);
                    if line_indent.len() > docstring_indent.len() {
                        let mut check = Check::new(
                            CheckKind::NoOverIndentation,
                            Range {
                                location: Location::new(docstring.location.row() + i, 0),
                                end_location: Location::new(docstring.location.row() + i, 0),
                            },
                        );
                        if checker.patch(check.kind.code()) {
                            check.amend(Fix::replacement(
                                whitespace::clean(&docstring_indent),
                                Location::new(docstring.location.row() + i, 0),
                                Location::new(docstring.location.row() + i, line_indent.len()),
                            ));
                        }
                        checker.add_check(check);
                    }
                }
            }
        }
    }
}

/// D209
pub fn newline_after_last_paragraph(checker: &mut Checker, definition: &Definition) {
    if let Some(docstring) = definition.docstring {
        if let ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } = &docstring.node
        {
            let mut line_count = 0;
            for line in string.lines() {
                if !line.trim().is_empty() {
                    line_count += 1;
                }
                if line_count > 1 {
                    let content = checker
                        .locator
                        .slice_source_code_range(&Range::from_located(docstring));
                    if let Some(last_line) = content.lines().last().map(|line| line.trim()) {
                        if last_line != "\"\"\"" && last_line != "'''" {
                            let mut check = Check::new(
                                CheckKind::NewLineAfterLastParagraph,
                                Range::from_located(docstring),
                            );
                            if checker.patch(check.kind.code()) {
                                // Insert a newline just before the end-quote(s).
                                let content = format!(
                                    "\n{}",
                                    whitespace::clean(&whitespace::indentation(checker, docstring))
                                );
                                check.amend(Fix::insertion(
                                    content,
                                    Location::new(
                                        docstring.end_location.unwrap().row(),
                                        docstring.end_location.unwrap().column() - "\"\"\"".len(),
                                    ),
                                ));
                            }
                            checker.add_check(check);
                        }
                    }
                    return;
                }
            }
        }
    }
}

/// D210
pub fn no_surrounding_whitespace(checker: &mut Checker, definition: &Definition) {
    if let Some(docstring) = definition.docstring {
        if let ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } = &docstring.node
        {
            let mut lines = string.lines();
            if let Some(line) = lines.next() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    return;
                }
                if line != trimmed {
                    let mut check = Check::new(
                        CheckKind::NoSurroundingWhitespace,
                        Range::from_located(docstring),
                    );
                    if checker.patch(check.kind.code()) {
                        if let Some(first_line) = checker
                            .locator
                            .slice_source_code_range(&Range::from_located(docstring))
                            .lines()
                            .next()
                            .map(|line| line.to_lowercase())
                        {
                            for pattern in constants::TRIPLE_QUOTE_PREFIXES
                                .iter()
                                .chain(constants::SINGLE_QUOTE_PREFIXES)
                            {
                                if first_line.starts_with(pattern) {
                                    check.amend(Fix::replacement(
                                        trimmed.to_string(),
                                        Location::new(
                                            docstring.location.row(),
                                            docstring.location.column() + pattern.len(),
                                        ),
                                        Location::new(
                                            docstring.location.row(),
                                            docstring.location.column()
                                                + pattern.len()
                                                + line.chars().count(),
                                        ),
                                    ));
                                    break;
                                }
                            }
                        }
                    }
                    checker.add_check(check);
                }
            }
        }
    }
}

/// D212, D213
pub fn multi_line_summary_start(checker: &mut Checker, definition: &Definition) {
    if let Some(docstring) = definition.docstring {
        if let ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } = &docstring.node
        {
            if string.lines().nth(1).is_some() {
                if let Some(first_line) = checker
                    .locator
                    .slice_source_code_range(&Range::from_located(docstring))
                    .lines()
                    .next()
                    .map(|line| line.to_lowercase())
                {
                    if constants::TRIPLE_QUOTE_PREFIXES.contains(&first_line.as_str()) {
                        if checker.settings.enabled.contains(&CheckCode::D212) {
                            checker.add_check(Check::new(
                                CheckKind::MultiLineSummaryFirstLine,
                                Range::from_located(docstring),
                            ));
                        }
                    } else {
                        if checker.settings.enabled.contains(&CheckCode::D213) {
                            checker.add_check(Check::new(
                                CheckKind::MultiLineSummarySecondLine,
                                Range::from_located(docstring),
                            ));
                        }
                    }
                }
            }
        }
    }
}

/// D300
pub fn triple_quotes(checker: &mut Checker, definition: &Definition) {
    if let Some(docstring) = definition.docstring {
        if let ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } = &docstring.node
        {
            if let Some(first_line) = checker
                .locator
                .slice_source_code_range(&Range::from_located(docstring))
                .lines()
                .next()
                .map(|line| line.to_lowercase())
            {
                let starts_with_triple = if string.contains("\"\"\"") {
                    first_line.starts_with("'''")
                        || first_line.starts_with("u'''")
                        || first_line.starts_with("r'''")
                        || first_line.starts_with("ur'''")
                } else {
                    first_line.starts_with("\"\"\"")
                        || first_line.starts_with("u\"\"\"")
                        || first_line.starts_with("r\"\"\"")
                        || first_line.starts_with("ur\"\"\"")
                };
                if !starts_with_triple {
                    checker.add_check(Check::new(
                        CheckKind::UsesTripleQuotes,
                        Range::from_located(docstring),
                    ));
                }
            }
        }
    }
}

/// D400
pub fn ends_with_period(checker: &mut Checker, definition: &Definition) {
    if let Some(docstring) = definition.docstring {
        if let ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } = &docstring.node
        {
            if let Some(string) = string.trim().lines().next() {
                if !string.ends_with('.') {
                    checker.add_check(Check::new(
                        CheckKind::EndsInPeriod,
                        Range::from_located(docstring),
                    ));
                }
            }
        }
    }
}

/// D402
pub fn no_signature(checker: &mut Checker, definition: &Definition) {
    if let Some(docstring) = definition.docstring {
        if let DefinitionKind::Function(parent)
        | DefinitionKind::NestedFunction(parent)
        | DefinitionKind::Method(parent) = definition.kind
        {
            if let StmtKind::FunctionDef { name, .. } = &parent.node {
                if let ExprKind::Constant {
                    value: Constant::Str(string),
                    ..
                } = &docstring.node
                {
                    if let Some(first_line) = string.lines().next() {
                        if first_line.contains(&format!("{name}(")) {
                            checker.add_check(Check::new(
                                CheckKind::NoSignature,
                                Range::from_located(docstring),
                            ));
                        }
                    }
                }
            }
        }
    }
}

/// D403
pub fn capitalized(checker: &mut Checker, definition: &Definition) {
    if !matches!(definition.kind, DefinitionKind::Function(_)) {
        return;
    }

    if let Some(docstring) = definition.docstring {
        if let ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } = &docstring.node
        {
            if let Some(first_word) = string.split(' ').next() {
                if first_word == first_word.to_uppercase() {
                    return;
                }
                for char in first_word.chars() {
                    if !char.is_ascii_alphabetic() && char != '\'' {
                        return;
                    }
                }
                if let Some(first_char) = first_word.chars().next() {
                    if !first_char.is_uppercase() {
                        checker.add_check(Check::new(
                            CheckKind::FirstLineCapitalized,
                            Range::from_located(docstring),
                        ));
                    }
                }
            }
        }
    }
}

/// D404
pub fn starts_with_this(checker: &mut Checker, definition: &Definition) {
    if let Some(docstring) = definition.docstring {
        if let ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } = &docstring.node
        {
            let trimmed = string.trim();
            if trimmed.is_empty() {
                return;
            }

            if let Some(first_word) = string.split(' ').next() {
                if first_word
                    .replace(|c: char| !c.is_alphanumeric(), "")
                    .to_lowercase()
                    == "this"
                {
                    checker.add_check(Check::new(
                        CheckKind::NoThisPrefix,
                        Range::from_located(docstring),
                    ));
                }
            }
        }
    }
}

/// D415
pub fn ends_with_punctuation(checker: &mut Checker, definition: &Definition) {
    if let Some(docstring) = definition.docstring {
        if let ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } = &docstring.node
        {
            if let Some(string) = string.trim().lines().next() {
                if !(string.ends_with('.') || string.ends_with('!') || string.ends_with('?')) {
                    checker.add_check(Check::new(
                        CheckKind::EndsInPunctuation,
                        Range::from_located(docstring),
                    ));
                }
            }
        }
    }
}

/// D418
pub fn if_needed(checker: &mut Checker, definition: &Definition) {
    if definition.docstring.is_some() {
        if let DefinitionKind::Function(stmt)
        | DefinitionKind::NestedFunction(stmt)
        | DefinitionKind::Method(stmt) = definition.kind
        {
            if is_overload(stmt) {
                checker.add_check(Check::new(
                    CheckKind::SkipDocstring,
                    Range::from_located(stmt),
                ));
            }
        }
    }
}

/// D419
pub fn not_empty(checker: &mut Checker, definition: &Definition) -> bool {
    if let Some(docstring) = definition.docstring {
        if let ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } = &docstring.node
        {
            if string.trim().is_empty() {
                if checker.settings.enabled.contains(&CheckCode::D419) {
                    checker.add_check(Check::new(
                        CheckKind::NonEmpty,
                        Range::from_located(docstring),
                    ));
                }
                return false;
            }
        }
    }
    true
}

/// D212, D214, D215, D405, D406, D407, D408, D409, D410, D411, D412, D413,
/// D414, D416, D417
pub fn sections(checker: &mut Checker, definition: &Definition) {
    if let Some(docstring) = definition.docstring {
        if let ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } = &docstring.node
        {
            let lines: Vec<&str> = string.lines().collect();
            if lines.len() < 2 {
                return;
            }

            // First, interpret as NumPy-style sections.
            let mut found_numpy_section = false;
            for context in &section_contexts(&lines, &SectionStyle::NumPy) {
                found_numpy_section = true;
                numpy_section(checker, definition, context);
            }

            // If no such sections were identified, interpret as Google-style sections.
            if !found_numpy_section {
                for context in &section_contexts(&lines, &SectionStyle::Google) {
                    google_section(checker, definition, context);
                }
            }
        }
    }
}

fn blanks_and_section_underline(
    checker: &mut Checker,
    definition: &Definition,
    context: &SectionContext,
) {
    let docstring = definition
        .docstring
        .expect("Sections are only available for docstrings.");

    let mut blank_lines_after_header = 0;
    for line in context.following_lines {
        if !line.trim().is_empty() {
            break;
        }
        blank_lines_after_header += 1;
    }

    // Nothing but blank lines after the section header.
    if blank_lines_after_header == context.following_lines.len() {
        if checker.settings.enabled.contains(&CheckCode::D407) {
            let mut check = Check::new(
                CheckKind::DashedUnderlineAfterSection(context.section_name.to_string()),
                Range::from_located(docstring),
            );
            if checker.patch(check.kind.code()) {
                // Add a dashed line (of the appropriate length) under the section header.
                let content = format!(
                    "{}{}\n",
                    whitespace::clean(&whitespace::indentation(checker, docstring)),
                    "-".repeat(context.section_name.len())
                );
                check.amend(Fix::insertion(
                    content,
                    Location::new(docstring.location.row() + context.original_index + 1, 0),
                ));
            }
            checker.add_check(check);
        }
        if checker.settings.enabled.contains(&CheckCode::D414) {
            checker.add_check(Check::new(
                CheckKind::NonEmptySection(context.section_name.to_string()),
                Range::from_located(docstring),
            ));
        }
        return;
    }

    let non_empty_line = context.following_lines[blank_lines_after_header];
    let dash_line_found = non_empty_line
        .chars()
        .all(|char| char.is_whitespace() || char == '-');

    if !dash_line_found {
        if checker.settings.enabled.contains(&CheckCode::D407) {
            let mut check = Check::new(
                CheckKind::DashedUnderlineAfterSection(context.section_name.to_string()),
                Range::from_located(docstring),
            );
            if checker.patch(check.kind.code()) {
                // Add a dashed line (of the appropriate length) under the section header.
                let content = format!(
                    "{}{}\n",
                    whitespace::clean(&whitespace::indentation(checker, docstring)),
                    "-".repeat(context.section_name.len())
                );
                check.amend(Fix::insertion(
                    content,
                    Location::new(docstring.location.row() + context.original_index + 1, 0),
                ));
            }
            checker.add_check(check);
        }
        if blank_lines_after_header > 0 {
            if checker.settings.enabled.contains(&CheckCode::D412) {
                let mut check = Check::new(
                    CheckKind::NoBlankLinesBetweenHeaderAndContent(
                        context.section_name.to_string(),
                    ),
                    Range::from_located(docstring),
                );
                if checker.patch(check.kind.code()) {
                    // Delete any blank lines between the header and content.
                    check.amend(Fix::deletion(
                        Location::new(docstring.location.row() + context.original_index + 1, 0),
                        Location::new(
                            docstring.location.row()
                                + context.original_index
                                + 1
                                + blank_lines_after_header,
                            0,
                        ),
                    ));
                }
                checker.add_check(check);
            }
        }
    } else {
        if blank_lines_after_header > 0 {
            if checker.settings.enabled.contains(&CheckCode::D408) {
                let mut check = Check::new(
                    CheckKind::SectionUnderlineAfterName(context.section_name.to_string()),
                    Range::from_located(docstring),
                );
                if checker.patch(check.kind.code()) {
                    // Delete any blank lines between the header and the underline.
                    check.amend(Fix::deletion(
                        Location::new(docstring.location.row() + context.original_index + 1, 0),
                        Location::new(
                            docstring.location.row()
                                + context.original_index
                                + 1
                                + blank_lines_after_header,
                            0,
                        ),
                    ));
                }
                checker.add_check(check);
            }
        }

        if non_empty_line
            .trim()
            .chars()
            .filter(|char| *char == '-')
            .count()
            != context.section_name.len()
        {
            if checker.settings.enabled.contains(&CheckCode::D409) {
                let mut check = Check::new(
                    CheckKind::SectionUnderlineMatchesSectionLength(
                        context.section_name.to_string(),
                    ),
                    Range::from_located(docstring),
                );
                if checker.patch(check.kind.code()) {
                    // Replace the existing underline with a line of the appropriate length.
                    let content = format!(
                        "{}{}\n",
                        whitespace::clean(&whitespace::indentation(checker, docstring)),
                        "-".repeat(context.section_name.len())
                    );
                    check.amend(Fix::replacement(
                        content,
                        Location::new(
                            docstring.location.row()
                                + context.original_index
                                + 1
                                + blank_lines_after_header,
                            0,
                        ),
                        Location::new(
                            docstring.location.row()
                                + context.original_index
                                + 1
                                + blank_lines_after_header
                                + 1,
                            0,
                        ),
                    ));
                };
                checker.add_check(check);
            }
        }

        if checker.settings.enabled.contains(&CheckCode::D215) {
            let leading_space = whitespace::leading_space(non_empty_line);
            let indentation = whitespace::indentation(checker, docstring);
            if leading_space.len() > indentation.len() {
                let mut check = Check::new(
                    CheckKind::SectionUnderlineNotOverIndented(context.section_name.to_string()),
                    Range::from_located(docstring),
                );
                if checker.patch(check.kind.code()) {
                    // Replace the existing indentation with whitespace of the appropriate length.
                    check.amend(Fix::replacement(
                        whitespace::clean(&indentation),
                        Location::new(
                            docstring.location.row()
                                + context.original_index
                                + 1
                                + blank_lines_after_header,
                            0,
                        ),
                        Location::new(
                            docstring.location.row()
                                + context.original_index
                                + 1
                                + blank_lines_after_header,
                            1 + leading_space.len(),
                        ),
                    ));
                };
                checker.add_check(check);
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
                    if checker.settings.enabled.contains(&CheckCode::D414) {
                        checker.add_check(Check::new(
                            CheckKind::NonEmptySection(context.section_name.to_string()),
                            Range::from_located(docstring),
                        ));
                    }
                } else {
                    if checker.settings.enabled.contains(&CheckCode::D412) {
                        let mut check = Check::new(
                            CheckKind::NoBlankLinesBetweenHeaderAndContent(
                                context.section_name.to_string(),
                            ),
                            Range::from_located(docstring),
                        );
                        if checker.patch(check.kind.code()) {
                            // Delete any blank lines between the header and content.
                            check.amend(Fix::deletion(
                                Location::new(
                                    docstring.location.row()
                                        + context.original_index
                                        + 1
                                        + line_after_dashes_index,
                                    0,
                                ),
                                Location::new(
                                    docstring.location.row()
                                        + context.original_index
                                        + 1
                                        + line_after_dashes_index
                                        + blank_lines_after_dashes,
                                    0,
                                ),
                            ));
                        }
                        checker.add_check(check);
                    }
                }
            }
        } else {
            if checker.settings.enabled.contains(&CheckCode::D414) {
                checker.add_check(Check::new(
                    CheckKind::NonEmptySection(context.section_name.to_string()),
                    Range::from_located(docstring),
                ));
            }
        }
    }
}

fn common_section(
    checker: &mut Checker,
    definition: &Definition,
    context: &SectionContext,
    style: &SectionStyle,
) {
    let docstring = definition
        .docstring
        .expect("Sections are only available for docstrings.");

    if checker.settings.enabled.contains(&CheckCode::D405) {
        if !style
            .section_names()
            .contains(&context.section_name.as_str())
        {
            let capitalized_section_name = titlecase::titlecase(&context.section_name);
            if style
                .section_names()
                .contains(capitalized_section_name.as_str())
            {
                let mut check = Check::new(
                    CheckKind::CapitalizeSectionName(context.section_name.to_string()),
                    Range::from_located(docstring),
                );
                if checker.patch(check.kind.code()) {
                    // Replace the section title with the capitalized variant. This requires
                    // locating the start and end of the section name.
                    if let Some(index) = context.line.find(&context.section_name) {
                        // Map from bytes to characters.
                        let section_name_start = &context.line[..index].chars().count();
                        let section_name_length = &context.section_name.chars().count();
                        check.amend(Fix::replacement(
                            capitalized_section_name,
                            Location::new(
                                docstring.location.row() + context.original_index,
                                *section_name_start,
                            ),
                            Location::new(
                                docstring.location.row() + context.original_index,
                                section_name_start + section_name_length,
                            ),
                        ))
                    }
                }
                checker.add_check(check);
            }
        }
    }

    if checker.settings.enabled.contains(&CheckCode::D214) {
        let leading_space = whitespace::leading_space(context.line);
        let indentation = whitespace::indentation(checker, docstring);
        if leading_space.len() > indentation.len() {
            let mut check = Check::new(
                CheckKind::SectionNotOverIndented(context.section_name.to_string()),
                Range::from_located(docstring),
            );
            if checker.patch(check.kind.code()) {
                // Replace the existing indentation with whitespace of the appropriate length.
                check.amend(Fix::replacement(
                    whitespace::clean(&indentation),
                    Location::new(docstring.location.row() + context.original_index, 0),
                    Location::new(
                        docstring.location.row() + context.original_index,
                        leading_space.len(),
                    ),
                ));
            };
            checker.add_check(check);
        }
    }

    if context
        .following_lines
        .last()
        .map(|line| !line.trim().is_empty())
        .unwrap_or(true)
    {
        if context.is_last_section {
            if checker.settings.enabled.contains(&CheckCode::D413) {
                let mut check = Check::new(
                    CheckKind::BlankLineAfterLastSection(context.section_name.to_string()),
                    Range::from_located(docstring),
                );
                if checker.patch(check.kind.code()) {
                    // Add a newline after the section.
                    check.amend(Fix::insertion(
                        "\n".to_string(),
                        Location::new(
                            docstring.location.row()
                                + context.original_index
                                + 1
                                + context.following_lines.len(),
                            0,
                        ),
                    ));
                }
                checker.add_check(check);
            }
        } else {
            if checker.settings.enabled.contains(&CheckCode::D410) {
                let mut check = Check::new(
                    CheckKind::BlankLineAfterSection(context.section_name.to_string()),
                    Range::from_located(docstring),
                );
                if checker.patch(check.kind.code()) {
                    // Add a newline after the section.
                    check.amend(Fix::insertion(
                        "\n".to_string(),
                        Location::new(
                            docstring.location.row()
                                + context.original_index
                                + 1
                                + context.following_lines.len(),
                            0,
                        ),
                    ));
                }
                checker.add_check(check);
            }
        }
    }

    if checker.settings.enabled.contains(&CheckCode::D411) {
        if !context.previous_line.is_empty() {
            let mut check = Check::new(
                CheckKind::BlankLineBeforeSection(context.section_name.to_string()),
                Range::from_located(docstring),
            );
            if checker.patch(check.kind.code()) {
                // Add a blank line before the section.
                check.amend(Fix::insertion(
                    "\n".to_string(),
                    Location::new(docstring.location.row() + context.original_index, 0),
                ));
            }
            checker.add_check(check)
        }
    }

    blanks_and_section_underline(checker, definition, context);
}

fn missing_args(
    checker: &mut Checker,
    definition: &Definition,
    docstrings_args: &FnvHashSet<&str>,
) {
    if let DefinitionKind::Function(parent)
    | DefinitionKind::NestedFunction(parent)
    | DefinitionKind::Method(parent) = definition.kind
    {
        if let StmtKind::FunctionDef {
            args: arguments, ..
        }
        | StmtKind::AsyncFunctionDef {
            args: arguments, ..
        } = &parent.node
        {
            // Collect all the arguments into a single vector.
            let mut all_arguments: Vec<&Arg> = arguments
                .args
                .iter()
                .chain(arguments.posonlyargs.iter())
                .chain(arguments.kwonlyargs.iter())
                .skip(
                    // If this is a non-static method, skip `cls` or `self`.
                    usize::from(
                        matches!(definition.kind, DefinitionKind::Method(_))
                            && !is_staticmethod(parent),
                    ),
                )
                .collect();
            if let Some(arg) = &arguments.vararg {
                all_arguments.push(arg);
            }
            if let Some(arg) = &arguments.kwarg {
                all_arguments.push(arg);
            }

            // Look for arguments that weren't included in the docstring.
            let mut missing_args: BTreeSet<&str> = Default::default();
            for arg in all_arguments {
                let arg_name = arg.node.arg.as_str();
                if arg_name.starts_with('_') {
                    continue;
                }
                if docstrings_args.contains(&arg_name) {
                    continue;
                }
                missing_args.insert(arg_name);
            }

            if !missing_args.is_empty() {
                let names = missing_args
                    .into_iter()
                    .map(String::from)
                    .sorted()
                    .collect();
                checker.add_check(Check::new(
                    CheckKind::DocumentAllArguments(names),
                    Range::from_located(parent),
                ));
            }
        }
    }
}

// See: `GOOGLE_ARGS_REGEX` in `pydocstyle/checker.py`.
static GOOGLE_ARGS_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(\w+)\s*(\(.*?\))?\s*:\n?\s*.+").expect("Invalid regex"));

fn args_section(checker: &mut Checker, definition: &Definition, context: &SectionContext) {
    let mut args_sections: Vec<String> = vec![];
    for line in textwrap::dedent(&context.following_lines.join("\n"))
        .trim()
        .lines()
    {
        if line
            .chars()
            .next()
            .map(|char| char.is_whitespace())
            .unwrap_or(true)
        {
            // This is a continuation of documentation for the last
            // parameter because it does start with whitespace.
            if let Some(current) = args_sections.last_mut() {
                current.push_str(line);
            }
        } else {
            // This line is the start of documentation for the next
            // parameter because it doesn't start with any whitespace.
            args_sections.push(line.to_string());
        }
    }

    missing_args(
        checker,
        definition,
        // Collect the list of arguments documented in the docstring.
        &FnvHashSet::from_iter(args_sections.iter().filter_map(|section| {
            match GOOGLE_ARGS_REGEX.captures(section.as_str()) {
                Some(caps) => caps.get(1).map(|arg_name| arg_name.as_str()),
                None => None,
            }
        })),
    )
}

fn parameters_section(checker: &mut Checker, definition: &Definition, context: &SectionContext) {
    // Collect the list of arguments documented in the docstring.
    let mut docstring_args: FnvHashSet<&str> = FnvHashSet::default();
    let section_level_indent = whitespace::leading_space(context.line);
    for i in 1..context.following_lines.len() {
        let current_line = context.following_lines[i - 1];
        let current_leading_space = whitespace::leading_space(current_line);
        let next_line = context.following_lines[i];
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
    missing_args(checker, definition, &docstring_args);
}

fn numpy_section(checker: &mut Checker, definition: &Definition, context: &SectionContext) {
    common_section(checker, definition, context, &SectionStyle::NumPy);

    if checker.settings.enabled.contains(&CheckCode::D406) {
        let suffix = context
            .line
            .trim()
            .strip_prefix(&context.section_name)
            .unwrap();
        if !suffix.is_empty() {
            let docstring = definition
                .docstring
                .expect("Sections are only available for docstrings.");
            let mut check = Check::new(
                CheckKind::NewLineAfterSectionName(context.section_name.to_string()),
                Range::from_located(docstring),
            );
            if checker.patch(check.kind.code()) {
                // Delete the suffix. This requires locating the end of the section name.
                if let Some(index) = context.line.find(&context.section_name) {
                    // Map from bytes to characters.
                    let suffix_start = &context.line[..index + context.section_name.len()]
                        .chars()
                        .count();
                    let suffix_length = suffix.chars().count();
                    check.amend(Fix::deletion(
                        Location::new(
                            docstring.location.row() + context.original_index,
                            *suffix_start,
                        ),
                        Location::new(
                            docstring.location.row() + context.original_index,
                            suffix_start + suffix_length,
                        ),
                    ));
                }
            }
            checker.add_check(check)
        }
    }

    if checker.settings.enabled.contains(&CheckCode::D417) {
        let capitalized_section_name = titlecase::titlecase(&context.section_name);
        if capitalized_section_name == "Parameters" {
            parameters_section(checker, definition, context);
        }
    }
}

fn google_section(checker: &mut Checker, definition: &Definition, context: &SectionContext) {
    common_section(checker, definition, context, &SectionStyle::Google);

    if checker.settings.enabled.contains(&CheckCode::D416) {
        let suffix = context
            .line
            .trim()
            .strip_prefix(&context.section_name)
            .unwrap();
        if suffix != ":" {
            let docstring = definition
                .docstring
                .expect("Sections are only available for docstrings.");
            let mut check = Check::new(
                CheckKind::SectionNameEndsInColon(context.section_name.to_string()),
                Range::from_located(docstring),
            );
            if checker.patch(check.kind.code()) {
                // Replace the suffix. This requires locating the end of the section name.
                if let Some(index) = context.line.find(&context.section_name) {
                    // Map from bytes to characters.
                    let suffix_start = &context.line[..index + context.section_name.len()]
                        .chars()
                        .count();
                    let suffix_length = suffix.chars().count();
                    check.amend(Fix::replacement(
                        ":".to_string(),
                        Location::new(
                            docstring.location.row() + context.original_index,
                            *suffix_start,
                        ),
                        Location::new(
                            docstring.location.row() + context.original_index,
                            suffix_start + suffix_length,
                        ),
                    ));
                }
            }
            checker.add_check(check);
        }
    }

    if checker.settings.enabled.contains(&CheckCode::D417) {
        let capitalized_section_name = titlecase::titlecase(&context.section_name);
        if capitalized_section_name == "Args" || capitalized_section_name == "Arguments" {
            args_section(checker, definition, context);
        }
    }
}
