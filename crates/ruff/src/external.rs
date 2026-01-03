use std::io::{self, Write};

use anyhow::Result;
use ruff_linter::external::ExternalLintRegistry;
use ruff_linter::external::ast::rule::{ExternalAstLinter, ExternalAstRule};
use ruff_linter::registry::Rule;
use ruff_workspace::Settings;
use rustc_hash::FxHashSet;

#[derive(Debug)]
pub(crate) struct ExternalSelectionState {
    pub ignored: FxHashSet<String>,
    pub effective: FxHashSet<String>,
}

pub(crate) fn compute_external_selection_state(
    base_selected: &[String],
    base_ignored: &[String],
    cli_select: &[String],
    cli_extend_select: &[String],
    cli_ignore: &[String],
    cli_extend_ignore: &[String],
) -> ExternalSelectionState {
    let mut selected: FxHashSet<String> = if cli_select.is_empty() {
        base_selected.iter().cloned().collect()
    } else {
        FxHashSet::default()
    };
    selected.extend(cli_select.iter().cloned());
    selected.extend(cli_extend_select.iter().cloned());

    let mut ignored: FxHashSet<String> = base_ignored.iter().cloned().collect();
    ignored.extend(cli_ignore.iter().cloned());
    ignored.extend(cli_extend_ignore.iter().cloned());

    let effective = selected
        .iter()
        .filter(|code| !ignored.contains(*code))
        .cloned()
        .collect();

    ExternalSelectionState { ignored, effective }
}

pub(crate) fn apply_external_linter_selection_to_settings(
    settings: &mut Settings,
    selected: &FxHashSet<String>,
    ignored: &FxHashSet<String>,
) -> Result<bool> {
    let linter = &mut settings.linter;

    if selected.is_empty() {
        if linter.rules.enabled(Rule::ExternalLinter) {
            linter.rules.disable(Rule::ExternalLinter);
        }
        linter.selected_external.clear();
        linter.external_ast = None;
        return Ok(true);
    }

    if !linter.rules.enabled(Rule::ExternalLinter) {
        linter.selected_external.clear();
        linter.external_ast = None;
        return Ok(false);
    }

    if let Some(registry) = linter.external_ast.take() {
        let selection = select_external_linters(&registry, selected, ignored);
        if !selection.missing.is_empty() {
            anyhow::bail!(
                "Unknown external linter or rule selector(s): {}",
                selection.missing.join(", ")
            );
        }

        let mut filtered = ExternalLintRegistry::new();
        for matched in &selection.matches {
            filtered.insert_linter(matched.clone_selected())?;
        }

        linter.selected_external = selected.iter().cloned().collect();
        let codes: Vec<String> = filtered
            .iter_enabled_rules()
            .map(|rule| rule.code.as_str().to_string())
            .collect();

        if filtered.is_empty() {
            linter.rules.disable(Rule::ExternalLinter);
            linter.external_ast = None;
            linter.selected_external.clear();
        } else {
            linter.rules.enable(Rule::ExternalLinter, false);
            linter.external_ast = Some(filtered);
            let external_codes = &mut linter.external;
            for code in codes {
                if !external_codes.iter().any(|existing| existing == &code) {
                    external_codes.push(code);
                }
            }
        }

        Ok(true)
    } else {
        Ok(false)
    }
}

pub(crate) fn select_external_linters<'a>(
    registry: &'a ExternalLintRegistry,
    selected: &FxHashSet<String>,
    ignored: &FxHashSet<String>,
) -> SelectedExternalLinters<'a> {
    let mut matches = Vec::new();
    let mut missing = Vec::new();

    let enabled_linters: Vec<&'a ExternalAstLinter> = registry
        .linters()
        .iter()
        .filter(|linter| linter.enabled)
        .collect();

    if selected.is_empty() {
        matches.extend(
            enabled_linters
                .iter()
                .copied()
                .map(SelectedExternalLinter::all_rules),
        );
        return SelectedExternalLinters { matches, missing };
    }

    let mut satisfied: FxHashSet<&'a str> = FxHashSet::default();
    let mut available_linter_ids: FxHashSet<&'a str> = FxHashSet::default();

    for linter in &enabled_linters {
        available_linter_ids.insert(linter.id.as_str());
    }

    for linter in enabled_linters {
        let selected_linter = selected.contains(linter.id.as_str());

        if selected_linter && ignored.is_empty() {
            matches.push(SelectedExternalLinter::all_rules(linter));
            satisfied.insert(linter.id.as_str());
            continue;
        }

        let included: Vec<_> = linter
            .rules
            .iter()
            .filter(|rule| !ignored.contains(rule.code.as_str()))
            .collect();

        if selected_linter {
            if included.is_empty() {
                missing.push(linter.id.clone());
                continue;
            }

            satisfied.insert(linter.id.as_str());
            for rule in &included {
                if selected.contains(rule.code.as_str()) {
                    satisfied.insert(rule.code.as_str());
                }
            }

            matches.push(SelectedExternalLinter::subset(linter, included));
            continue;
        }

        let matched_rules: Vec<_> = included
            .iter()
            .copied()
            .filter(|rule| selected.contains(rule.code.as_str()))
            .collect();

        if matched_rules.is_empty() {
            continue;
        }

        for rule in &matched_rules {
            satisfied.insert(rule.code.as_str());
        }

        matches.push(SelectedExternalLinter::subset(linter, matched_rules));
    }

    for selector in selected {
        let selector = selector.as_str();
        if ignored.contains(selector) || satisfied.contains(selector) {
            continue;
        }

        if available_linter_ids.contains(selector) {
            continue;
        }

        if registry.find_rule_by_code(selector).is_some() {
            continue;
        }

        missing.push(selector.to_string());
    }

    SelectedExternalLinters { matches, missing }
}

#[derive(Debug)]
pub(crate) struct SelectedExternalLinters<'a> {
    pub matches: Vec<SelectedExternalLinter<'a>>,
    pub missing: Vec<String>,
}

pub(crate) fn print_external_linters(
    registry: &ExternalLintRegistry,
    linters: &[SelectedExternalLinter<'_>],
    mut writer: impl Write,
) -> io::Result<()> {
    match (registry.is_empty(), linters.is_empty()) {
        (true, _) => writeln!(writer, "No external AST linters configured.")?,
        (false, true) => writeln!(writer, "No matching external AST linters found.")?,
        (false, false) => {
            for selected in linters {
                selected.print(&mut writer)?;
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) struct SelectedExternalLinter<'a> {
    linter: &'a ExternalAstLinter,
    selection: SelectedRules<'a>,
}

impl<'a> SelectedExternalLinter<'a> {
    fn all_rules(linter: &'a ExternalAstLinter) -> Self {
        Self {
            linter,
            selection: SelectedRules::All,
        }
    }

    fn subset(linter: &'a ExternalAstLinter, rules: Vec<&'a ExternalAstRule>) -> Self {
        debug_assert!(!rules.is_empty());
        Self {
            linter,
            selection: SelectedRules::Subset(rules),
        }
    }

    fn clone_selected(&self) -> ExternalAstLinter {
        match &self.selection {
            SelectedRules::All => self.linter.clone(),
            SelectedRules::Subset(rules) => ExternalAstLinter {
                id: self.linter.id.clone(),
                name: self.linter.name.clone(),
                description: self.linter.description.clone(),
                enabled: self.linter.enabled,
                rules: rules.iter().map(|&rule| rule.clone()).collect(),
            },
        }
    }

    fn print(&self, writer: &mut impl Write) -> io::Result<()> {
        match &self.selection {
            SelectedRules::All => write!(writer, "{}", self.linter),
            SelectedRules::Subset(rules) => {
                writeln!(
                    writer,
                    "{}{}",
                    self.linter.id,
                    if self.linter.enabled {
                        ""
                    } else {
                        " (disabled)"
                    }
                )?;
                writeln!(writer, "    name: {}", self.linter.name)?;
                if let Some(description) = &self.linter.description {
                    writeln!(writer, "    description: {description}")?;
                }
                writeln!(writer, "    rules:")?;
                for rule in rules {
                    writeln!(writer, "      - {} ({})", rule.code.as_str(), rule.name)?;
                }
                writeln!(writer)
            }
        }
    }
}

#[derive(Debug)]
enum SelectedRules<'a> {
    All,
    Subset(Vec<&'a ExternalAstRule>),
}
