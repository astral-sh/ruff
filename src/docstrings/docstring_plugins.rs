//! Abstractions for tracking and validating docstrings in Python code.

use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_ast::{Constant, ExprKind, Location, StmtKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckCode, CheckKind};
use crate::docstrings::google::check_google_section;
use crate::docstrings::helpers;
use crate::docstrings::numpy::check_numpy_section;
use crate::docstrings::sections::section_contexts;
use crate::docstrings::styles::SectionStyle;
use crate::docstrings::types::{Definition, DefinitionKind};
use crate::visibility::{is_init, is_magic, is_overload, Visibility};

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
                        location: Location::new(1, 1),
                        end_location: Location::new(1, 1),
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
                        location: Location::new(1, 1),
                        end_location: Location::new(1, 1),
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
                    helpers::range_for(docstring),
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
                let (before, _, after) = checker.locator.partition_source_code_at(
                    &Range::from_located(parent),
                    &helpers::range_for(docstring),
                );

                if checker.settings.enabled.contains(&CheckCode::D201) {
                    let blank_lines_before = before
                        .lines()
                        .rev()
                        .skip(1)
                        .take_while(|line| line.trim().is_empty())
                        .count();
                    if blank_lines_before != 0 {
                        checker.add_check(Check::new(
                            CheckKind::NoBlankLineBeforeFunction(blank_lines_before),
                            helpers::range_for(docstring),
                        ));
                    }
                }

                if checker.settings.enabled.contains(&CheckCode::D202) {
                    let blank_lines_after = after
                        .lines()
                        .skip(1)
                        .take_while(|line| line.trim().is_empty())
                        .count();
                    let all_blank_after = after
                        .lines()
                        .skip(1)
                        .all(|line| line.trim().is_empty() || COMMENT_REGEX.is_match(line));
                    // Report a D202 violation if the docstring is followed by a blank line
                    // and the blank line is not itself followed by an inner function or
                    // class.
                    if !all_blank_after
                        && blank_lines_after != 0
                        && !(blank_lines_after == 1
                            && INNER_FUNCTION_OR_CLASS_REGEX.is_match(after))
                    {
                        checker.add_check(Check::new(
                            CheckKind::NoBlankLineAfterFunction(blank_lines_after),
                            helpers::range_for(docstring),
                        ));
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
                let (before, _, after) = checker.locator.partition_source_code_at(
                    &Range::from_located(parent),
                    &helpers::range_for(docstring),
                );

                if checker.settings.enabled.contains(&CheckCode::D203)
                    || checker.settings.enabled.contains(&CheckCode::D211)
                {
                    let blank_lines_before = before
                        .lines()
                        .rev()
                        .skip(1)
                        .take_while(|line| line.trim().is_empty())
                        .count();
                    if blank_lines_before != 0
                        && checker.settings.enabled.contains(&CheckCode::D211)
                    {
                        checker.add_check(Check::new(
                            CheckKind::NoBlankLineBeforeClass(blank_lines_before),
                            helpers::range_for(docstring),
                        ));
                    }
                    if blank_lines_before != 1
                        && checker.settings.enabled.contains(&CheckCode::D203)
                    {
                        checker.add_check(Check::new(
                            CheckKind::OneBlankLineBeforeClass(blank_lines_before),
                            helpers::range_for(docstring),
                        ));
                    }
                }

                if checker.settings.enabled.contains(&CheckCode::D204) {
                    let blank_lines_after = after
                        .lines()
                        .skip(1)
                        .take_while(|line| line.trim().is_empty())
                        .count();
                    let all_blank_after = after
                        .lines()
                        .skip(1)
                        .all(|line| line.trim().is_empty() || COMMENT_REGEX.is_match(line));
                    if !all_blank_after && blank_lines_after != 1 {
                        checker.add_check(Check::new(
                            CheckKind::OneBlankLineAfterClass(blank_lines_after),
                            helpers::range_for(docstring),
                        ));
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
                checker.add_check(Check::new(
                    CheckKind::NoBlankLineAfterSummary,
                    helpers::range_for(docstring),
                ));
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
                        .slice_source_code_range(&helpers::range_for(docstring));
                    if let Some(line) = content.lines().last() {
                        let line = line.trim();
                        if line != "\"\"\"" && line != "'''" {
                            checker.add_check(Check::new(
                                CheckKind::NewLineAfterLastParagraph,
                                helpers::range_for(docstring),
                            ));
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
                if line.trim().is_empty() {
                    return;
                }
                if line.starts_with(' ') || (matches!(lines.next(), None) && line.ends_with(' ')) {
                    checker.add_check(Check::new(
                        CheckKind::NoSurroundingWhitespace,
                        helpers::range_for(docstring),
                    ));
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
                let content = checker
                    .locator
                    .slice_source_code_range(&helpers::range_for(docstring));
                if let Some(first_line) = content.lines().next() {
                    let first_line = first_line.trim();
                    if first_line == "\"\"\"" || first_line == "'''" {
                        if checker.settings.enabled.contains(&CheckCode::D212) {
                            checker.add_check(Check::new(
                                CheckKind::MultiLineSummaryFirstLine,
                                helpers::range_for(docstring),
                            ));
                        }
                    } else if checker.settings.enabled.contains(&CheckCode::D213) {
                        checker.add_check(Check::new(
                            CheckKind::MultiLineSummarySecondLine,
                            helpers::range_for(docstring),
                        ));
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
            let content = checker
                .locator
                .slice_source_code_range(&helpers::range_for(docstring));
            if string.contains("\"\"\"") {
                if !content.starts_with("'''") {
                    checker.add_check(Check::new(
                        CheckKind::UsesTripleQuotes,
                        helpers::range_for(docstring),
                    ));
                }
            } else if !content.starts_with("\"\"\"") {
                checker.add_check(Check::new(
                    CheckKind::UsesTripleQuotes,
                    helpers::range_for(docstring),
                ));
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
            if let Some(string) = string.lines().next() {
                if !string.ends_with('.') {
                    checker.add_check(Check::new(
                        CheckKind::EndsInPeriod,
                        helpers::range_for(docstring),
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
                                helpers::range_for(docstring),
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
                            helpers::range_for(docstring),
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
                        helpers::range_for(docstring),
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
            if let Some(string) = string.lines().next() {
                if !(string.ends_with('.') || string.ends_with('!') || string.ends_with('?')) {
                    checker.add_check(Check::new(
                        CheckKind::EndsInPunctuation,
                        helpers::range_for(docstring),
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
                        helpers::range_for(docstring),
                    ));
                }
                return false;
            }
        }
    }
    true
}

/// D212, D214, D215, D405, D406, D407, D408, D409, D410, D411, D412, D413, D414, D416, D417
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
                check_numpy_section(checker, definition, context);
            }

            // If no such sections were identified, interpret as Google-style sections.
            if !found_numpy_section {
                for context in &section_contexts(&lines, &SectionStyle::Google) {
                    check_google_section(checker, definition, context);
                }
            }
        }
    }
}
