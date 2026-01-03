use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::external::PyprojectExternalLinterEntry;
use crate::external::ast::definition::ExternalAstLinterFile;
use crate::external::ast::registry::ExternalLintRegistry;
use crate::external::ast::rule::{
    CallCalleeMatcher, ExternalAstLinter, ExternalAstRule, ExternalAstRuleSpec, ExternalRuleCode,
    ExternalRuleCodeError, ExternalRuleScript,
};
use crate::external::ast::target::{AstTarget, AstTargetSpec, ExprKind};
use crate::external::error::ExternalLinterError;

pub fn load_linter_into_registry(
    registry: &mut ExternalLintRegistry,
    id: &str,
    entry: &PyprojectExternalLinterEntry,
) -> Result<(), ExternalLinterError> {
    let linter = load_linter_from_entry(id, entry)?;
    registry.insert_linter(linter)
}

pub fn load_linter_from_entry(
    id: &str,
    entry: &PyprojectExternalLinterEntry,
) -> Result<ExternalAstLinter, ExternalLinterError> {
    let definition = load_definition_file(&entry.toml_path)?;
    build_linter(id, entry, &definition)
}

fn load_definition_file(path: &Path) -> Result<ExternalAstLinterFile, ExternalLinterError> {
    let contents = fs::read_to_string(path).map_err(|source| ExternalLinterError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&contents).map_err(|source| ExternalLinterError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

fn build_linter(
    id: &str,
    entry: &PyprojectExternalLinterEntry,
    linter_file: &ExternalAstLinterFile,
) -> Result<ExternalAstLinter, ExternalLinterError> {
    if linter_file.rules.is_empty() {
        return Err(ExternalLinterError::EmptyLinter { id: id.to_string() });
    }

    let resolved_dir = entry
        .toml_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let mut codes = HashSet::new();
    let mut rules = Vec::with_capacity(linter_file.rules.len());

    for rule_spec in &linter_file.rules {
        let rule = build_rule(id, &resolved_dir, rule_spec)?;

        if !codes.insert(rule.code.as_str().to_string()) {
            return Err(ExternalLinterError::DuplicateRule {
                linter: id.to_string(),
                code: rule.code.as_str().to_string(),
            });
        }

        rules.push(rule);
    }

    let linter = ExternalAstLinter::new(
        id,
        linter_file.name.clone().unwrap_or_else(|| id.to_string()),
        linter_file.description.clone(),
        entry.enabled && linter_file.enabled,
        rules,
    );

    Ok(linter)
}

fn build_rule(
    linter_id: &str,
    base_dir: &Path,
    spec: &ExternalAstRuleSpec,
) -> Result<ExternalAstRule, ExternalLinterError> {
    let code = ExternalRuleCode::new(&spec.code).map_err(|error| match error {
        ExternalRuleCodeError::Empty | ExternalRuleCodeError::InvalidCharacters(_) => {
            ExternalLinterError::InvalidRuleCode {
                linter: linter_id.to_string(),
                code: spec.code.clone(),
            }
        }
    })?;

    if spec.targets.is_empty() {
        return Err(ExternalLinterError::MissingTargets {
            linter: linter_id.to_string(),
            rule: spec.name.clone(),
        });
    }

    let mut resolved_targets = Vec::with_capacity(spec.targets.len());
    for target in &spec.targets {
        let parsed = parse_target(target).map_err(|source| ExternalLinterError::UnknownTarget {
            linter: linter_id.to_string(),
            rule: spec.name.clone(),
            target: target.raw().to_string(),
            source,
        })?;
        resolved_targets.push(parsed);
    }

    let script = resolve_script(linter_id, &spec.name, base_dir, &spec.script)?;
    let call_callee = if let Some(pattern) = spec.call_callee_regex.as_ref() {
        if !resolved_targets
            .iter()
            .any(|target| matches!(target, AstTarget::Expr(ExprKind::Call)))
        {
            return Err(ExternalLinterError::CallCalleeRegexWithoutCallTarget {
                linter: linter_id.to_string(),
                rule: spec.name.clone(),
            });
        }

        Some(CallCalleeMatcher::new(pattern.clone()).map_err(|source| {
            ExternalLinterError::InvalidCallCalleeRegex {
                linter: linter_id.to_string(),
                rule: spec.name.clone(),
                pattern: pattern.clone(),
                source,
            }
        })?)
    } else {
        None
    };

    Ok(ExternalAstRule::new(
        code,
        spec.name.clone(),
        spec.summary.clone(),
        resolved_targets,
        script,
        call_callee,
    ))
}

fn resolve_script(
    linter_id: &str,
    rule_name: &str,
    base_dir: &Path,
    script_path: &Path,
) -> Result<ExternalRuleScript, ExternalLinterError> {
    let resolved = if script_path.is_absolute() {
        script_path.to_path_buf()
    } else {
        base_dir.join(script_path)
    };
    let contents =
        fs::read_to_string(&resolved).map_err(|source| ExternalLinterError::ScriptIo {
            linter: linter_id.to_string(),
            rule: rule_name.to_string(),
            path: resolved.clone(),
            source,
        })?;
    if contents.trim().is_empty() {
        return Err(ExternalLinterError::MissingScriptBody {
            linter: linter_id.to_string(),
            rule: rule_name.to_string(),
        });
    }
    Ok(ExternalRuleScript::file(resolved, contents))
}

fn parse_target(
    spec: &AstTargetSpec,
) -> Result<AstTarget, crate::external::ast::target::AstTargetParseError> {
    spec.parse()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::external::ast::target::{ExprKind, StmtKind};
    use anyhow::Result;
    use std::path::Path;
    use tempfile::tempdir;

    fn write(path: &Path, contents: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, contents)?;
        Ok(())
    }

    #[test]
    fn load_linter_from_entry_resolves_relative_paths() -> Result<()> {
        let temp = tempdir()?;
        let linter_path = temp.path().join("linters/my_linter.toml");
        let script_path = temp.path().join("linters/rules/example.py");
        let call_script_path = temp.path().join("linters/rules/call.py");

        write(
            &script_path,
            r#"
def check():
    # placeholder body
    pass
"#,
        )?;

        write(
            &call_script_path,
            r#"
def check():
    pass
"#,
        )?;

        write(
            &linter_path,
            r#"
name = "Example External Linter"
description = "Demonstrates external AST configuration"

[[rule]]
code = "EXT001"
name = "ExampleRule"
summary = "Flags demo targets"
targets = ["stmt:FunctionDef"]
script = "rules/example.py"

[[rule]]
code = "EXT100"
name = "CallRule"
targets = ["expr:Call"]
call-callee-regex = "^logging\\."
script = "rules/call.py"
"#,
        )?;

        let entry = PyprojectExternalLinterEntry {
            toml_path: linter_path,
            enabled: true,
        };

        let linter = load_linter_from_entry("example", &entry)?;
        assert!(linter.enabled);
        assert_eq!(linter.id.as_str(), "example");
        assert_eq!(linter.name.as_str(), "Example External Linter");
        assert_eq!(
            linter.description.as_deref(),
            Some("Demonstrates external AST configuration")
        );
        assert_eq!(linter.rules.len(), 2);

        let example_rule = &linter.rules[0];
        assert_eq!(example_rule.code.as_str(), "EXT001");
        assert_eq!(example_rule.name.as_str(), "ExampleRule");
        assert_eq!(example_rule.summary.as_deref(), Some("Flags demo targets"));
        assert_eq!(example_rule.targets.len(), 1);
        assert_eq!(
            example_rule.targets[0],
            AstTarget::Stmt(StmtKind::FunctionDef)
        );
        assert_eq!(example_rule.script.path(), script_path.as_path());
        assert!(example_rule.script.body().contains("placeholder body"));

        let call_rule = &linter.rules[1];
        assert_eq!(call_rule.code.as_str(), "EXT100");
        assert_eq!(call_rule.name.as_str(), "CallRule");
        assert_eq!(call_rule.targets[0], AstTarget::Expr(ExprKind::Call));
        let call_callee = call_rule
            .call_callee()
            .expect("expected call callee matcher to be present");
        assert_eq!(call_callee.pattern(), "^logging\\.");
        assert!(call_callee.regex().is_match("logging.info"));

        Ok(())
    }

    #[test]
    fn load_linter_rejects_call_regex_without_call_target() -> Result<()> {
        let temp = tempdir()?;
        let linter_path = temp.path().join("linters/invalid-call.toml");
        let script_path = temp.path().join("linters/rules/invalid.py");

        write(
            &script_path,
            r#"
def check():
    pass
"#,
        )?;

        write(
            &linter_path,
            r#"
[[rule]]
code = "EXT101"
name = "InvalidCallRule"
targets = ["expr:Name"]
call-callee-regex = "^logging\\."
script = "rules/invalid.py"
"#,
        )?;

        let entry = PyprojectExternalLinterEntry {
            toml_path: linter_path,
            enabled: true,
        };

        let err = load_linter_from_entry("invalid-call", &entry).unwrap_err();
        let ExternalLinterError::CallCalleeRegexWithoutCallTarget { linter, rule } = err else {
            panic!("expected call regex without target error");
        };
        assert_eq!(linter, "invalid-call");
        assert_eq!(rule, "InvalidCallRule");

        Ok(())
    }

    #[test]
    fn load_linter_rejects_invalid_call_regex() -> Result<()> {
        let temp = tempdir()?;
        let linter_path = temp.path().join("linters/bad-regex.toml");
        let script_path = temp.path().join("linters/rules/bad.py");

        write(
            &script_path,
            r#"
def check():
    pass
"#,
        )?;

        write(
            &linter_path,
            r#"
[[rule]]
code = "EXT102"
name = "BadRegexRule"
targets = ["expr:Call"]
call-callee-regex = "["
script = "rules/bad.py"
"#,
        )?;

        let entry = PyprojectExternalLinterEntry {
            toml_path: linter_path,
            enabled: true,
        };

        let err = load_linter_from_entry("bad-regex", &entry).unwrap_err();
        let ExternalLinterError::InvalidCallCalleeRegex {
            linter,
            rule,
            pattern,
            ..
        } = err
        else {
            panic!("expected invalid call regex error");
        };
        assert_eq!(linter, "bad-regex");
        assert_eq!(rule, "BadRegexRule");
        assert_eq!(pattern, "[");

        Ok(())
    }

    #[test]
    fn load_linter_into_registry_marks_disabled_linters() -> Result<()> {
        let temp = tempdir()?;
        let linter_path = temp.path().join("linters/disabled.toml");
        let script_path = temp.path().join("linters/rules/unused.py");

        write(
            &script_path,
            r#"
def check():
    pass
"#,
        )?;

        write(
            &linter_path,
            r#"
enabled = false

[[rule]]
code = "EXT002"
name = "DisabledRule"
targets = ["stmt:Expr"]
script = "rules/unused.py"
"#,
        )?;

        let entry = PyprojectExternalLinterEntry {
            toml_path: linter_path,
            enabled: true,
        };

        let mut registry = ExternalLintRegistry::new();
        load_linter_into_registry(&mut registry, "disabled", &entry)?;

        assert_eq!(registry.linters().len(), 1);

        let linter = &registry.linters()[0];
        assert!(!linter.enabled);

        // Disabled linters should not be discoverable by rule code lookup.
        assert!(registry.find_rule_by_code("EXT002").is_none());

        Ok(())
    }
}
