use itertools::Itertools;
use once_cell::sync::Lazy;
use regex::Regex;
use rustc_hash::FxHashSet;
use rustpython_ast::{Location, StmtKind};

use crate::ast::helpers::identifier_range;
use crate::ast::types::Range;
use crate::ast::whitespace::LinesWithTrailingNewline;
use crate::ast::{cast, whitespace};
use crate::autofix::Fix;
use crate::docstrings::constants;
use crate::docstrings::definition::{Definition, DefinitionKind, Docstring};
use crate::docstrings::sections::{section_contexts, SectionContext};
use crate::docstrings::styles::SectionStyle;
use crate::pydocstyle::helpers::{leading_quote, logical_line};
use crate::pydocstyle::settings::Convention;
use crate::registry::{Diagnostic, RuleCode};
use crate::violations;
use crate::visibility::{is_init, is_magic, is_overload, is_override, is_staticmethod, Visibility};
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// D100, D101, D102, D103, D104, D105, D106, D107
pub fn not_missing(
    xxxxxxxx: &mut xxxxxxxx,
    definition: &Definition,
    visibility: &Visibility,
) -> bool {
    if matches!(visibility, Visibility::Private) {
        return true;
    }

    match definition.kind {
        DefinitionKind::Module => {
            if xxxxxxxx.settings.enabled.contains(&RuleCode::D100) {
                xxxxxxxx.diagnostics.push(Diagnostic::new(
                    violations::PublicModule,
                    Range::new(Location::new(1, 0), Location::new(1, 0)),
                ));
            }
            false
        }
        DefinitionKind::Package => {
            if xxxxxxxx.settings.enabled.contains(&RuleCode::D104) {
                xxxxxxxx.diagnostics.push(Diagnostic::new(
                    violations::PublicPackage,
                    Range::new(Location::new(1, 0), Location::new(1, 0)),
                ));
            }
            false
        }
        DefinitionKind::Class(stmt) => {
            if xxxxxxxx.settings.enabled.contains(&RuleCode::D101) {
                xxxxxxxx.diagnostics.push(Diagnostic::new(
                    violations::PublicClass,
                    identifier_range(stmt, xxxxxxxx.locator),
                ));
            }
            false
        }
        DefinitionKind::NestedClass(stmt) => {
            if xxxxxxxx.settings.enabled.contains(&RuleCode::D106) {
                xxxxxxxx.diagnostics.push(Diagnostic::new(
                    violations::PublicNestedClass,
                    identifier_range(stmt, xxxxxxxx.locator),
                ));
            }
            false
        }
        DefinitionKind::Function(stmt) | DefinitionKind::NestedFunction(stmt) => {
            if is_overload(xxxxxxxx, cast::decorator_list(stmt)) {
                true
            } else {
                if xxxxxxxx.settings.enabled.contains(&RuleCode::D103) {
                    xxxxxxxx.diagnostics.push(Diagnostic::new(
                        violations::PublicFunction,
                        identifier_range(stmt, xxxxxxxx.locator),
                    ));
                }
                false
            }
        }
        DefinitionKind::Method(stmt) => {
            if is_overload(xxxxxxxx, cast::decorator_list(stmt))
                || is_override(xxxxxxxx, cast::decorator_list(stmt))
            {
                true
            } else if is_magic(stmt) {
                if xxxxxxxx.settings.enabled.contains(&RuleCode::D105) {
                    xxxxxxxx.diagnostics.push(Diagnostic::new(
                        violations::MagicMethod,
                        identifier_range(stmt, xxxxxxxx.locator),
                    ));
                }
                true
            } else if is_init(stmt) {
                if xxxxxxxx.settings.enabled.contains(&RuleCode::D107) {
                    xxxxxxxx.diagnostics.push(Diagnostic::new(
                        violations::PublicInit,
                        identifier_range(stmt, xxxxxxxx.locator),
                    ));
                }
                true
            } else {
                if xxxxxxxx.settings.enabled.contains(&RuleCode::D102) {
                    xxxxxxxx.diagnostics.push(Diagnostic::new(
                        violations::PublicMethod,
                        identifier_range(stmt, xxxxxxxx.locator),
                    ));
                }
                true
            }
        }
    }
}

/// D200
pub fn one_liner(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring) {
    let body = docstring.body;

    let mut line_count = 0;
    let mut non_empty_line_count = 0;
    for line in LinesWithTrailingNewline::from(body) {
        line_count += 1;
        if !line.trim().is_empty() {
            non_empty_line_count += 1;
        }
        if non_empty_line_count > 1 {
            break;
        }
    }

    if non_empty_line_count == 1 && line_count > 1 {
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::FitsOnOneLine,
            Range::from_located(docstring.expr),
        ));
    }
}

static COMMENT_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*#").unwrap());

static INNER_FUNCTION_OR_CLASS_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s+(?:(?:class|def|async def)\s|@)").unwrap());

/// D201, D202
pub fn blank_before_after_function(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring) {
    let (
        DefinitionKind::Function(parent)
        | DefinitionKind::NestedFunction(parent)
        | DefinitionKind::Method(parent)
    ) = &docstring.kind else {
        return;
    };

    if xxxxxxxx.settings.enabled.contains(&RuleCode::D201) {
        let (before, ..) = xxxxxxxx.locator.partition_source_code_at(
            &Range::from_located(parent),
            &Range::from_located(docstring.expr),
        );

        let blank_lines_before = before
            .lines()
            .rev()
            .skip(1)
            .take_while(|line| line.trim().is_empty())
            .count();
        if blank_lines_before != 0 {
            let mut check = Diagnostic::new(
                violations::NoBlankLineBeforeFunction(blank_lines_before),
                Range::from_located(docstring.expr),
            );
            if xxxxxxxx.patch(check.kind.code()) {
                // Delete the blank line before the docstring.
                check.amend(Fix::deletion(
                    Location::new(docstring.expr.location.row() - blank_lines_before, 0),
                    Location::new(docstring.expr.location.row(), 0),
                ));
            }
            xxxxxxxx.diagnostics.push(check);
        }
    }

    if xxxxxxxx.settings.enabled.contains(&RuleCode::D202) {
        let (_, _, after) = xxxxxxxx.locator.partition_source_code_at(
            &Range::from_located(parent),
            &Range::from_located(docstring.expr),
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
            let mut check = Diagnostic::new(
                violations::NoBlankLineAfterFunction(blank_lines_after),
                Range::from_located(docstring.expr),
            );
            if xxxxxxxx.patch(check.kind.code()) {
                // Delete the blank line after the docstring.
                check.amend(Fix::deletion(
                    Location::new(docstring.expr.end_location.unwrap().row() + 1, 0),
                    Location::new(
                        docstring.expr.end_location.unwrap().row() + 1 + blank_lines_after,
                        0,
                    ),
                ));
            }
            xxxxxxxx.diagnostics.push(check);
        }
    }
}

/// D203, D204, D211
pub fn blank_before_after_class(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring) {
    let (DefinitionKind::Class(parent) | DefinitionKind::NestedClass(parent)) = &docstring.kind else {
        return;
    };

    if xxxxxxxx.settings.enabled.contains(&RuleCode::D203)
        || xxxxxxxx.settings.enabled.contains(&RuleCode::D211)
    {
        let (before, ..) = xxxxxxxx.locator.partition_source_code_at(
            &Range::from_located(parent),
            &Range::from_located(docstring.expr),
        );

        let blank_lines_before = before
            .lines()
            .rev()
            .skip(1)
            .take_while(|line| line.trim().is_empty())
            .count();
        if xxxxxxxx.settings.enabled.contains(&RuleCode::D211) {
            if blank_lines_before != 0 {
                let mut check = Diagnostic::new(
                    violations::NoBlankLineBeforeClass(blank_lines_before),
                    Range::from_located(docstring.expr),
                );
                if xxxxxxxx.patch(check.kind.code()) {
                    // Delete the blank line before the class.
                    check.amend(Fix::deletion(
                        Location::new(docstring.expr.location.row() - blank_lines_before, 0),
                        Location::new(docstring.expr.location.row(), 0),
                    ));
                }
                xxxxxxxx.diagnostics.push(check);
            }
        }
        if xxxxxxxx.settings.enabled.contains(&RuleCode::D203) {
            if blank_lines_before != 1 {
                let mut check = Diagnostic::new(
                    violations::OneBlankLineBeforeClass(blank_lines_before),
                    Range::from_located(docstring.expr),
                );
                if xxxxxxxx.patch(check.kind.code()) {
                    // Insert one blank line before the class.
                    check.amend(Fix::replacement(
                        "\n".to_string(),
                        Location::new(docstring.expr.location.row() - blank_lines_before, 0),
                        Location::new(docstring.expr.location.row(), 0),
                    ));
                }
                xxxxxxxx.diagnostics.push(check);
            }
        }
    }

    if xxxxxxxx.settings.enabled.contains(&RuleCode::D204) {
        let (_, _, after) = xxxxxxxx.locator.partition_source_code_at(
            &Range::from_located(parent),
            &Range::from_located(docstring.expr),
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
            let mut check = Diagnostic::new(
                violations::OneBlankLineAfterClass(blank_lines_after),
                Range::from_located(docstring.expr),
            );
            if xxxxxxxx.patch(check.kind.code()) {
                // Insert a blank line before the class (replacing any existing lines).
                check.amend(Fix::replacement(
                    "\n".to_string(),
                    Location::new(docstring.expr.end_location.unwrap().row() + 1, 0),
                    Location::new(
                        docstring.expr.end_location.unwrap().row() + 1 + blank_lines_after,
                        0,
                    ),
                ));
            }
            xxxxxxxx.diagnostics.push(check);
        }
    }
}

/// D205
pub fn blank_after_summary(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring) {
    let body = docstring.body;

    let mut lines_count = 1;
    let mut blanks_count = 0;
    for line in body.trim().lines().skip(1) {
        lines_count += 1;
        if line.trim().is_empty() {
            blanks_count += 1;
        } else {
            break;
        }
    }
    if lines_count > 1 && blanks_count != 1 {
        let mut check = Diagnostic::new(
            violations::BlankLineAfterSummary(blanks_count),
            Range::from_located(docstring.expr),
        );
        if xxxxxxxx.patch(check.kind.code()) {
            if blanks_count > 1 {
                // Find the "summary" line (defined as the first non-blank line).
                let mut summary_line = 0;
                for line in body.lines() {
                    if line.trim().is_empty() {
                        summary_line += 1;
                    } else {
                        break;
                    }
                }

                // Insert one blank line after the summary (replacing any existing lines).
                check.amend(Fix::replacement(
                    "\n".to_string(),
                    Location::new(docstring.expr.location.row() + summary_line + 1, 0),
                    Location::new(
                        docstring.expr.location.row() + summary_line + 1 + blanks_count,
                        0,
                    ),
                ));
            }
        }
        xxxxxxxx.diagnostics.push(check);
    }
}

/// D206, D207, D208
pub fn indent(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring) {
    let body = docstring.body;

    // Split the docstring into lines.
    let lines: Vec<&str> = LinesWithTrailingNewline::from(body).collect();
    if lines.len() <= 1 {
        return;
    }

    let mut has_seen_tab = docstring.indentation.contains('\t');
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

        if xxxxxxxx.settings.enabled.contains(&RuleCode::D207) {
            // We report under-indentation on every line. This isn't great, but enables
            // autofix.
            if (i == lines.len() - 1 || !is_blank)
                && line_indent.len() < docstring.indentation.len()
            {
                let mut check = Diagnostic::new(
                    violations::NoUnderIndentation,
                    Range::new(
                        Location::new(docstring.expr.location.row() + i, 0),
                        Location::new(docstring.expr.location.row() + i, 0),
                    ),
                );
                if xxxxxxxx.patch(check.kind.code()) {
                    check.amend(Fix::replacement(
                        whitespace::clean(docstring.indentation),
                        Location::new(docstring.expr.location.row() + i, 0),
                        Location::new(docstring.expr.location.row() + i, line_indent.len()),
                    ));
                }
                xxxxxxxx.diagnostics.push(check);
            }
        }

        // Like pydocstyle, we only report over-indentation if either: (1) every line
        // (except, optionally, the last line) is over-indented, or (2) the last line
        // (which contains the closing quotation marks) is
        // over-indented. We can't know if we've achieved that condition
        // until we've viewed all the lines, so for now, just track
        // the over-indentation status of every line.
        if i < lines.len() - 1 {
            if line_indent.len() > docstring.indentation.len() {
                over_indented_lines.push(i);
            } else {
                is_over_indented = false;
            }
        }
    }

    if xxxxxxxx.settings.enabled.contains(&RuleCode::D206) {
        if has_seen_tab {
            xxxxxxxx.diagnostics.push(Diagnostic::new(
                violations::IndentWithSpaces,
                Range::from_located(docstring.expr),
            ));
        }
    }

    if xxxxxxxx.settings.enabled.contains(&RuleCode::D208) {
        // If every line (except the last) is over-indented...
        if is_over_indented {
            for i in over_indented_lines {
                let line_indent = whitespace::leading_space(lines[i]);
                if line_indent.len() > docstring.indentation.len() {
                    // We report over-indentation on every line. This isn't great, but
                    // enables autofix.
                    let mut check = Diagnostic::new(
                        violations::NoOverIndentation,
                        Range::new(
                            Location::new(docstring.expr.location.row() + i, 0),
                            Location::new(docstring.expr.location.row() + i, 0),
                        ),
                    );
                    if xxxxxxxx.patch(check.kind.code()) {
                        check.amend(Fix::replacement(
                            whitespace::clean(docstring.indentation),
                            Location::new(docstring.expr.location.row() + i, 0),
                            Location::new(docstring.expr.location.row() + i, line_indent.len()),
                        ));
                    }
                    xxxxxxxx.diagnostics.push(check);
                }
            }
        }

        // If the last line is over-indented...
        if !lines.is_empty() {
            let i = lines.len() - 1;
            let line_indent = whitespace::leading_space(lines[i]);
            if line_indent.len() > docstring.indentation.len() {
                let mut check = Diagnostic::new(
                    violations::NoOverIndentation,
                    Range::new(
                        Location::new(docstring.expr.location.row() + i, 0),
                        Location::new(docstring.expr.location.row() + i, 0),
                    ),
                );
                if xxxxxxxx.patch(check.kind.code()) {
                    check.amend(Fix::replacement(
                        whitespace::clean(docstring.indentation),
                        Location::new(docstring.expr.location.row() + i, 0),
                        Location::new(docstring.expr.location.row() + i, line_indent.len()),
                    ));
                }
                xxxxxxxx.diagnostics.push(check);
            }
        }
    }
}

/// D209
pub fn newline_after_last_paragraph(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring) {
    let contents = docstring.contents;
    let body = docstring.body;

    let mut line_count = 0;
    for line in LinesWithTrailingNewline::from(body) {
        if !line.trim().is_empty() {
            line_count += 1;
        }
        if line_count > 1 {
            if let Some(last_line) = contents.lines().last().map(str::trim) {
                if last_line != "\"\"\"" && last_line != "'''" {
                    let mut check = Diagnostic::new(
                        violations::NewLineAfterLastParagraph,
                        Range::from_located(docstring.expr),
                    );
                    if xxxxxxxx.patch(check.kind.code()) {
                        // Insert a newline just before the end-quote(s).
                        let content = format!("\n{}", whitespace::clean(docstring.indentation));
                        check.amend(Fix::insertion(
                            content,
                            Location::new(
                                docstring.expr.end_location.unwrap().row(),
                                docstring.expr.end_location.unwrap().column() - "\"\"\"".len(),
                            ),
                        ));
                    }
                    xxxxxxxx.diagnostics.push(check);
                }
            }
            return;
        }
    }
}

/// D210
pub fn no_surrounding_whitespace(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring) {
    let contents = docstring.contents;
    let body = docstring.body;

    let mut lines = LinesWithTrailingNewline::from(body);
    let Some(line) = lines.next() else {
        return;
    };
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    if line == trimmed {
        return;
    }
    let mut check = Diagnostic::new(
        violations::NoSurroundingWhitespace,
        Range::from_located(docstring.expr),
    );
    if xxxxxxxx.patch(check.kind.code()) {
        if let Some(pattern) = leading_quote(contents) {
            if let Some(quote) = pattern.chars().last() {
                // If removing whitespace would lead to an invalid string of quote
                // characters, avoid applying the fix.
                if !trimmed.ends_with(quote) {
                    check.amend(Fix::replacement(
                        trimmed.to_string(),
                        Location::new(
                            docstring.expr.location.row(),
                            docstring.expr.location.column() + pattern.len(),
                        ),
                        Location::new(
                            docstring.expr.location.row(),
                            docstring.expr.location.column() + pattern.len() + line.chars().count(),
                        ),
                    ));
                }
            }
        }
    }
    xxxxxxxx.diagnostics.push(check);
}

/// D212, D213
pub fn multi_line_summary_start(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring) {
    let contents = docstring.contents;
    let body = docstring.body;

    if LinesWithTrailingNewline::from(body).nth(1).is_none() {
        return;
    };
    let Some(first_line) = contents
        .lines()
        .next()
         else
    {
        return;
    };
    if constants::TRIPLE_QUOTE_PREFIXES.contains(&first_line) {
        if xxxxxxxx.settings.enabled.contains(&RuleCode::D212) {
            xxxxxxxx.diagnostics.push(Diagnostic::new(
                violations::MultiLineSummaryFirstLine,
                Range::from_located(docstring.expr),
            ));
        }
    } else {
        if xxxxxxxx.settings.enabled.contains(&RuleCode::D213) {
            xxxxxxxx.diagnostics.push(Diagnostic::new(
                violations::MultiLineSummarySecondLine,
                Range::from_located(docstring.expr),
            ));
        }
    }
}

/// D300
pub fn triple_quotes(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring) {
    let contents = docstring.contents;
    let body = docstring.body;

    let Some(first_line) = contents
        .lines()
        .next()
        .map(str::to_lowercase) else
    {
        return;
    };
    let starts_with_triple = if body.contains("\"\"\"") {
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
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::UsesTripleQuotes,
            Range::from_located(docstring.expr),
        ));
    }
}

static BACKSLASH_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\\[^\nuN]").unwrap());

/// D301
pub fn backslashes(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring) {
    let contents = docstring.contents;

    // Docstring is already raw.
    if contents.starts_with('r') || contents.starts_with("ur") {
        return;
    }

    if BACKSLASH_REGEX.is_match(contents) {
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::UsesRPrefixForBackslashedContent,
            Range::from_located(docstring.expr),
        ));
    }
}

/// D400
pub fn ends_with_period(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring) {
    let contents = docstring.contents;
    let body = docstring.body;

    if let Some(first_line) = body.trim().lines().next() {
        let trimmed = first_line.trim();

        // Avoid false-positives: `:param`, etc.
        for prefix in [":param", ":type", ":raises", ":return", ":rtype"] {
            if trimmed.starts_with(prefix) {
                return;
            }
        }

        // Avoid false-positives: `Args:`, etc.
        for style in [SectionStyle::Google, SectionStyle::Numpy] {
            for section_name in style.section_names().iter() {
                if let Some(suffix) = trimmed.strip_suffix(section_name) {
                    if suffix.is_empty() {
                        return;
                    }
                    if suffix == ":" {
                        return;
                    }
                }
            }
        }
    }

    if let Some(index) = logical_line(body) {
        let line = body.lines().nth(index).unwrap();
        let trimmed = line.trim_end();

        if !trimmed.ends_with('.') {
            let mut check = Diagnostic::new(
                violations::EndsInPeriod,
                Range::from_located(docstring.expr),
            );
            // Best-effort autofix: avoid adding a period after other punctuation marks.
            if xxxxxxxx.patch(&RuleCode::D400) && !trimmed.ends_with(':') && !trimmed.ends_with(';')
            {
                if let Some((row, column)) = if index == 0 {
                    leading_quote(contents).map(|pattern| {
                        (
                            docstring.expr.location.row(),
                            docstring.expr.location.column()
                                + pattern.len()
                                + trimmed.chars().count(),
                        )
                    })
                } else {
                    Some((
                        docstring.expr.location.row() + index,
                        trimmed.chars().count(),
                    ))
                } {
                    check.amend(Fix::insertion(".".to_string(), Location::new(row, column)));
                }
            }
            xxxxxxxx.diagnostics.push(check);
        };
    }
}

/// D402
pub fn no_signature(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring) {
    let (
        DefinitionKind::Function(parent)
        | DefinitionKind::NestedFunction(parent)
        | DefinitionKind::Method(parent)
    ) = docstring.kind else {
        return;
    };
    let StmtKind::FunctionDef { name, .. } = &parent.node else {
        return;
    };

    let body = docstring.body;

    let Some(first_line) = body.trim().lines().next() else {
        return;
    };
    if !first_line.contains(&format!("{name}(")) {
        return;
    };
    xxxxxxxx.diagnostics.push(Diagnostic::new(
        violations::NoSignature,
        Range::from_located(docstring.expr),
    ));
}

/// D403
pub fn capitalized(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring) {
    if !matches!(docstring.kind, DefinitionKind::Function(_)) {
        return;
    }

    let body = docstring.body;

    let Some(first_word) = body.split(' ').next() else {
        return
    };
    if first_word == first_word.to_uppercase() {
        return;
    }
    for char in first_word.chars() {
        if !char.is_ascii_alphabetic() && char != '\'' {
            return;
        }
    }
    let Some(first_char) = first_word.chars().next() else {
        return;
    };
    if first_char.is_uppercase() {
        return;
    };
    xxxxxxxx.diagnostics.push(Diagnostic::new(
        violations::FirstLineCapitalized,
        Range::from_located(docstring.expr),
    ));
}

/// D404
pub fn starts_with_this(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring) {
    let body = docstring.body;

    let trimmed = body.trim();
    if trimmed.is_empty() {
        return;
    }

    let Some(first_word) = body.split(' ').next() else {
        return
    };
    if first_word
        .replace(|c: char| !c.is_alphanumeric(), "")
        .to_lowercase()
        != "this"
    {
        return;
    }
    xxxxxxxx.diagnostics.push(Diagnostic::new(
        violations::NoThisPrefix,
        Range::from_located(docstring.expr),
    ));
}

/// D415
pub fn ends_with_punctuation(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring) {
    let contents = docstring.contents;
    let body = docstring.body;

    if let Some(first_line) = body.trim().lines().next() {
        let trimmed = first_line.trim();

        // Avoid false-positives: `:param`, etc.
        for prefix in [":param", ":type", ":raises", ":return", ":rtype"] {
            if trimmed.starts_with(prefix) {
                return;
            }
        }

        // Avoid false-positives: `Args:`, etc.
        for style in [SectionStyle::Google, SectionStyle::Numpy] {
            for section_name in style.section_names().iter() {
                if let Some(suffix) = trimmed.strip_suffix(section_name) {
                    if suffix.is_empty() {
                        return;
                    }
                    if suffix == ":" {
                        return;
                    }
                }
            }
        }
    }

    if let Some(index) = logical_line(body) {
        let line = body.lines().nth(index).unwrap();
        let trimmed = line.trim_end();
        if !(trimmed.ends_with('.') || trimmed.ends_with('!') || trimmed.ends_with('?')) {
            let mut check = Diagnostic::new(
                violations::EndsInPunctuation,
                Range::from_located(docstring.expr),
            );
            // Best-effort autofix: avoid adding a period after other punctuation marks.
            if xxxxxxxx.patch(&RuleCode::D415) && !trimmed.ends_with(':') && !trimmed.ends_with(';')
            {
                if let Some((row, column)) = if index == 0 {
                    leading_quote(contents).map(|pattern| {
                        (
                            docstring.expr.location.row(),
                            docstring.expr.location.column()
                                + pattern.len()
                                + trimmed.chars().count(),
                        )
                    })
                } else {
                    Some((
                        docstring.expr.location.row() + index,
                        trimmed.chars().count(),
                    ))
                } {
                    check.amend(Fix::insertion(".".to_string(), Location::new(row, column)));
                }
            }
            xxxxxxxx.diagnostics.push(check);
        };
    }
}

/// D418
pub fn if_needed(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring) {
    let (
        DefinitionKind::Function(stmt)
        | DefinitionKind::NestedFunction(stmt)
        | DefinitionKind::Method(stmt)
    ) = docstring.kind else {
        return
    };
    if !is_overload(xxxxxxxx, cast::decorator_list(stmt)) {
        return;
    }
    xxxxxxxx.diagnostics.push(Diagnostic::new(
        violations::SkipDocstring,
        identifier_range(stmt, xxxxxxxx.locator),
    ));
}

/// D419
pub fn not_empty(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring) -> bool {
    if !docstring.body.trim().is_empty() {
        return true;
    }

    if xxxxxxxx.settings.enabled.contains(&RuleCode::D419) {
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::NonEmpty,
            Range::from_located(docstring.expr),
        ));
    }
    false
}

/// D212, D214, D215, D405, D406, D407, D408, D409, D410, D411, D412, D413,
/// D414, D416, D417
pub fn sections(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring, convention: Option<&Convention>) {
    let body = docstring.body;

    let lines: Vec<&str> = LinesWithTrailingNewline::from(body).collect();
    if lines.len() < 2 {
        return;
    }

    match convention {
        Some(Convention::Google) => {
            for context in &section_contexts(&lines, &SectionStyle::Google) {
                google_section(xxxxxxxx, docstring, context);
            }
        }
        Some(Convention::Numpy) => {
            for context in &section_contexts(&lines, &SectionStyle::Numpy) {
                numpy_section(xxxxxxxx, docstring, context);
            }
        }
        Some(Convention::Pep257) | None => {
            // First, interpret as NumPy-style sections.
            let mut found_numpy_section = false;
            for context in &section_contexts(&lines, &SectionStyle::Numpy) {
                found_numpy_section = true;
                numpy_section(xxxxxxxx, docstring, context);
            }

            // If no such sections were identified, interpret as Google-style sections.
            if !found_numpy_section {
                for context in &section_contexts(&lines, &SectionStyle::Google) {
                    google_section(xxxxxxxx, docstring, context);
                }
            }
        }
    }
}

fn blanks_and_section_underline(
    xxxxxxxx: &mut xxxxxxxx,
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
        if xxxxxxxx.settings.enabled.contains(&RuleCode::D407) {
            let mut check = Diagnostic::new(
                violations::DashedUnderlineAfterSection(context.section_name.to_string()),
                Range::from_located(docstring.expr),
            );
            if xxxxxxxx.patch(check.kind.code()) {
                // Add a dashed line (of the appropriate length) under the section header.
                let content = format!(
                    "{}{}\n",
                    whitespace::clean(docstring.indentation),
                    "-".repeat(context.section_name.len())
                );
                check.amend(Fix::insertion(
                    content,
                    Location::new(
                        docstring.expr.location.row() + context.original_index + 1,
                        0,
                    ),
                ));
            }
            xxxxxxxx.diagnostics.push(check);
        }
        if xxxxxxxx.settings.enabled.contains(&RuleCode::D414) {
            xxxxxxxx.diagnostics.push(Diagnostic::new(
                violations::NonEmptySection(context.section_name.to_string()),
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
            if xxxxxxxx.settings.enabled.contains(&RuleCode::D408) {
                let mut check = Diagnostic::new(
                    violations::SectionUnderlineAfterName(context.section_name.to_string()),
                    Range::from_located(docstring.expr),
                );
                if xxxxxxxx.patch(check.kind.code()) {
                    // Delete any blank lines between the header and the underline.
                    check.amend(Fix::deletion(
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
                xxxxxxxx.diagnostics.push(check);
            }
        }

        if non_empty_line
            .trim()
            .chars()
            .filter(|char| *char == '-')
            .count()
            != context.section_name.len()
        {
            if xxxxxxxx.settings.enabled.contains(&RuleCode::D409) {
                let mut check = Diagnostic::new(
                    violations::SectionUnderlineMatchesSectionLength(
                        context.section_name.to_string(),
                    ),
                    Range::from_located(docstring.expr),
                );
                if xxxxxxxx.patch(check.kind.code()) {
                    // Replace the existing underline with a line of the appropriate length.
                    let content = format!(
                        "{}{}\n",
                        whitespace::clean(docstring.indentation),
                        "-".repeat(context.section_name.len())
                    );
                    check.amend(Fix::replacement(
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
                xxxxxxxx.diagnostics.push(check);
            }
        }

        if xxxxxxxx.settings.enabled.contains(&RuleCode::D215) {
            let leading_space = whitespace::leading_space(non_empty_line);
            if leading_space.len() > docstring.indentation.len() {
                let mut check = Diagnostic::new(
                    violations::SectionUnderlineNotOverIndented(context.section_name.to_string()),
                    Range::from_located(docstring.expr),
                );
                if xxxxxxxx.patch(check.kind.code()) {
                    // Replace the existing indentation with whitespace of the appropriate length.
                    check.amend(Fix::replacement(
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
                xxxxxxxx.diagnostics.push(check);
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
                    if xxxxxxxx.settings.enabled.contains(&RuleCode::D414) {
                        xxxxxxxx.diagnostics.push(Diagnostic::new(
                            violations::NonEmptySection(context.section_name.to_string()),
                            Range::from_located(docstring.expr),
                        ));
                    }
                } else {
                    if xxxxxxxx.settings.enabled.contains(&RuleCode::D412) {
                        let mut check = Diagnostic::new(
                            violations::NoBlankLinesBetweenHeaderAndContent(
                                context.section_name.to_string(),
                            ),
                            Range::from_located(docstring.expr),
                        );
                        if xxxxxxxx.patch(check.kind.code()) {
                            // Delete any blank lines between the header and content.
                            check.amend(Fix::deletion(
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
                        xxxxxxxx.diagnostics.push(check);
                    }
                }
            }
        } else {
            if xxxxxxxx.settings.enabled.contains(&RuleCode::D414) {
                xxxxxxxx.diagnostics.push(Diagnostic::new(
                    violations::NonEmptySection(context.section_name.to_string()),
                    Range::from_located(docstring.expr),
                ));
            }
        }
    } else {
        if xxxxxxxx.settings.enabled.contains(&RuleCode::D407) {
            let mut check = Diagnostic::new(
                violations::DashedUnderlineAfterSection(context.section_name.to_string()),
                Range::from_located(docstring.expr),
            );
            if xxxxxxxx.patch(check.kind.code()) {
                // Add a dashed line (of the appropriate length) under the section header.
                let content = format!(
                    "{}{}\n",
                    whitespace::clean(docstring.indentation),
                    "-".repeat(context.section_name.len())
                );
                check.amend(Fix::insertion(
                    content,
                    Location::new(
                        docstring.expr.location.row() + context.original_index + 1,
                        0,
                    ),
                ));
            }
            xxxxxxxx.diagnostics.push(check);
        }
        if blank_lines_after_header > 0 {
            if xxxxxxxx.settings.enabled.contains(&RuleCode::D412) {
                let mut check = Diagnostic::new(
                    violations::NoBlankLinesBetweenHeaderAndContent(
                        context.section_name.to_string(),
                    ),
                    Range::from_located(docstring.expr),
                );
                if xxxxxxxx.patch(check.kind.code()) {
                    // Delete any blank lines between the header and content.
                    check.amend(Fix::deletion(
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
                xxxxxxxx.diagnostics.push(check);
            }
        }
    }
}

fn common_section(
    xxxxxxxx: &mut xxxxxxxx,
    docstring: &Docstring,
    context: &SectionContext,
    style: &SectionStyle,
) {
    if xxxxxxxx.settings.enabled.contains(&RuleCode::D405) {
        if !style.section_names().contains(&context.section_name) {
            let capitalized_section_name = titlecase::titlecase(context.section_name);
            if style
                .section_names()
                .contains(capitalized_section_name.as_str())
            {
                let mut check = Diagnostic::new(
                    violations::CapitalizeSectionName(context.section_name.to_string()),
                    Range::from_located(docstring.expr),
                );
                if xxxxxxxx.patch(check.kind.code()) {
                    // Replace the section title with the capitalized variant. This requires
                    // locating the start and end of the section name.
                    if let Some(index) = context.line.find(context.section_name) {
                        // Map from bytes to characters.
                        let section_name_start = &context.line[..index].chars().count();
                        let section_name_length = &context.section_name.chars().count();
                        check.amend(Fix::replacement(
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
                xxxxxxxx.diagnostics.push(check);
            }
        }
    }

    if xxxxxxxx.settings.enabled.contains(&RuleCode::D214) {
        let leading_space = whitespace::leading_space(context.line);
        if leading_space.len() > docstring.indentation.len() {
            let mut check = Diagnostic::new(
                violations::SectionNotOverIndented(context.section_name.to_string()),
                Range::from_located(docstring.expr),
            );
            if xxxxxxxx.patch(check.kind.code()) {
                // Replace the existing indentation with whitespace of the appropriate length.
                check.amend(Fix::replacement(
                    whitespace::clean(docstring.indentation),
                    Location::new(docstring.expr.location.row() + context.original_index, 0),
                    Location::new(
                        docstring.expr.location.row() + context.original_index,
                        leading_space.len(),
                    ),
                ));
            };
            xxxxxxxx.diagnostics.push(check);
        }
    }

    if context
        .following_lines
        .last()
        .map_or(true, |line| !line.trim().is_empty())
    {
        if context.is_last_section {
            if xxxxxxxx.settings.enabled.contains(&RuleCode::D413) {
                let mut check = Diagnostic::new(
                    violations::BlankLineAfterLastSection(context.section_name.to_string()),
                    Range::from_located(docstring.expr),
                );
                if xxxxxxxx.patch(check.kind.code()) {
                    // Add a newline after the section.
                    check.amend(Fix::insertion(
                        "\n".to_string(),
                        Location::new(
                            docstring.expr.location.row()
                                + context.original_index
                                + 1
                                + context.following_lines.len(),
                            0,
                        ),
                    ));
                }
                xxxxxxxx.diagnostics.push(check);
            }
        } else {
            if xxxxxxxx.settings.enabled.contains(&RuleCode::D410) {
                let mut check = Diagnostic::new(
                    violations::BlankLineAfterSection(context.section_name.to_string()),
                    Range::from_located(docstring.expr),
                );
                if xxxxxxxx.patch(check.kind.code()) {
                    // Add a newline after the section.
                    check.amend(Fix::insertion(
                        "\n".to_string(),
                        Location::new(
                            docstring.expr.location.row()
                                + context.original_index
                                + 1
                                + context.following_lines.len(),
                            0,
                        ),
                    ));
                }
                xxxxxxxx.diagnostics.push(check);
            }
        }
    }

    if xxxxxxxx.settings.enabled.contains(&RuleCode::D411) {
        if !context.previous_line.is_empty() {
            let mut check = Diagnostic::new(
                violations::BlankLineBeforeSection(context.section_name.to_string()),
                Range::from_located(docstring.expr),
            );
            if xxxxxxxx.patch(check.kind.code()) {
                // Add a blank line before the section.
                check.amend(Fix::insertion(
                    "\n".to_string(),
                    Location::new(docstring.expr.location.row() + context.original_index, 0),
                ));
            }
            xxxxxxxx.diagnostics.push(check);
        }
    }

    blanks_and_section_underline(xxxxxxxx, docstring, context);
}

fn missing_args(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring, docstrings_args: &FxHashSet<&str>) {
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
                    && !is_staticmethod(xxxxxxxx, cast::decorator_list(parent)),
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
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::DocumentAllArguments(names),
            Range::from_located(parent),
        ));
    }
}

// See: `GOOGLE_ARGS_REGEX` in `pydocstyle/xxxxxxxx.py`.
static GOOGLE_ARGS_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(\*?\*?\w+)\s*(\(.*?\))?\s*:\n?\s*.+").unwrap());

fn args_section(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring, context: &SectionContext) {
    if context.following_lines.is_empty() {
        missing_args(xxxxxxxx, docstring, &FxHashSet::default());
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

    missing_args(xxxxxxxx, docstring, &docstrings_args);
}

fn parameters_section(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring, context: &SectionContext) {
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
    missing_args(xxxxxxxx, docstring, &docstring_args);
}

fn numpy_section(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring, context: &SectionContext) {
    common_section(xxxxxxxx, docstring, context, &SectionStyle::Numpy);

    if xxxxxxxx.settings.enabled.contains(&RuleCode::D406) {
        let suffix = context
            .line
            .trim()
            .strip_prefix(context.section_name)
            .unwrap();
        if !suffix.is_empty() {
            let mut check = Diagnostic::new(
                violations::NewLineAfterSectionName(context.section_name.to_string()),
                Range::from_located(docstring.expr),
            );
            if xxxxxxxx.patch(check.kind.code()) {
                // Delete the suffix. This requires locating the end of the section name.
                if let Some(index) = context.line.find(context.section_name) {
                    // Map from bytes to characters.
                    let suffix_start = &context.line[..index + context.section_name.len()]
                        .chars()
                        .count();
                    let suffix_length = suffix.chars().count();
                    check.amend(Fix::deletion(
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
            xxxxxxxx.diagnostics.push(check);
        }
    }

    if xxxxxxxx.settings.enabled.contains(&RuleCode::D417) {
        let capitalized_section_name = titlecase::titlecase(context.section_name);
        if capitalized_section_name == "Parameters" {
            parameters_section(xxxxxxxx, docstring, context);
        }
    }
}

fn google_section(xxxxxxxx: &mut xxxxxxxx, docstring: &Docstring, context: &SectionContext) {
    common_section(xxxxxxxx, docstring, context, &SectionStyle::Google);

    if xxxxxxxx.settings.enabled.contains(&RuleCode::D416) {
        let suffix = context
            .line
            .trim()
            .strip_prefix(context.section_name)
            .unwrap();
        if suffix != ":" {
            let mut check = Diagnostic::new(
                violations::SectionNameEndsInColon(context.section_name.to_string()),
                Range::from_located(docstring.expr),
            );
            if xxxxxxxx.patch(check.kind.code()) {
                // Replace the suffix. This requires locating the end of the section name.
                if let Some(index) = context.line.find(context.section_name) {
                    // Map from bytes to characters.
                    let suffix_start = &context.line[..index + context.section_name.len()]
                        .chars()
                        .count();
                    let suffix_length = suffix.chars().count();
                    check.amend(Fix::replacement(
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
            xxxxxxxx.diagnostics.push(check);
        }
    }

    if xxxxxxxx.settings.enabled.contains(&RuleCode::D417) {
        let capitalized_section_name = titlecase::titlecase(context.section_name);
        if capitalized_section_name == "Args" || capitalized_section_name == "Arguments" {
            args_section(xxxxxxxx, docstring, context);
        }
    }
}
