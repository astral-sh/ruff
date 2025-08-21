use std::collections::HashSet;
use std::io::Write;

use anyhow::Result;
use serde::{Serialize, Serializer};
use serde_json::json;

use ruff_db::diagnostic::{Diagnostic, SecondaryCode};
use ruff_source_file::OneIndexed;

use crate::VERSION;
use crate::fs::normalize_path;
use crate::message::{Emitter, EmitterContext};
use crate::registry::{Linter, RuleNamespace};

pub struct SarifEmitter;

impl Emitter for SarifEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        diagnostics: &[Diagnostic],
        _context: &EmitterContext,
    ) -> Result<()> {
        let results = diagnostics
            .iter()
            .map(SarifResult::from_message)
            .collect::<Result<Vec<_>>>()?;

        let unique_rules: HashSet<_> = results
            .iter()
            .filter_map(|result| result.code.as_secondary_code())
            .collect();
        let mut rules: Vec<SarifRule> = unique_rules.into_iter().map(SarifRule::from).collect();
        rules.sort_by(|a, b| a.code.cmp(b.code));

        let output = json!({
            "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
            "version": "2.1.0",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": "ruff",
                        "informationUri": "https://github.com/astral-sh/ruff",
                        "rules": rules,
                        "version": VERSION.to_string(),
                    }
                },
                "results": results,
            }],
        });
        serde_json::to_writer_pretty(writer, &output)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct SarifRule<'a> {
    name: &'a str,
    code: &'a SecondaryCode,
    linter: &'a str,
    summary: &'a str,
    explanation: Option<&'a str>,
    url: Option<String>,
}

impl<'a> From<&'a SecondaryCode> for SarifRule<'a> {
    fn from(code: &'a SecondaryCode) -> Self {
        // This is a manual re-implementation of Rule::from_code, but we also want the Linter. This
        // avoids calling Linter::parse_code twice.
        let (linter, suffix) = Linter::parse_code(code).unwrap();
        let rule = linter
            .all_rules()
            .find(|rule| rule.noqa_code().suffix() == suffix)
            .expect("Expected a valid noqa code corresponding to a rule");
        Self {
            name: rule.into(),
            code,
            linter: linter.name(),
            summary: rule.message_formats()[0],
            explanation: rule.explanation(),
            url: rule.url(),
        }
    }
}

impl Serialize for SarifRule<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        json!({
            "id": self.code,
            "shortDescription": {
                "text": self.summary,
            },
            "fullDescription": {
                "text": self.explanation,
            },
            "help": {
                "text": self.summary,
            },
            "helpUri": self.url,
            "properties": {
                "id": self.code,
                "kind": self.linter,
                "name": self.name,
                "problem.severity": "error".to_string(),
            },
        })
        .serialize(serializer)
    }
}

#[derive(Debug)]
enum RuleCode<'a> {
    SecondaryCode(&'a SecondaryCode),
    LintId(&'a str),
}

impl RuleCode<'_> {
    fn as_secondary_code(&self) -> Option<&SecondaryCode> {
        match self {
            RuleCode::SecondaryCode(code) => Some(code),
            RuleCode::LintId(_) => None,
        }
    }

    fn as_str(&self) -> &str {
        match self {
            RuleCode::SecondaryCode(code) => code.as_str(),
            RuleCode::LintId(id) => id,
        }
    }
}

impl<'a> From<&'a Diagnostic> for RuleCode<'a> {
    fn from(code: &'a Diagnostic) -> Self {
        match code.secondary_code() {
            Some(diagnostic) => Self::SecondaryCode(diagnostic),
            None => Self::LintId(code.id().as_str()),
        }
    }
}

#[derive(Debug)]
struct SarifResult<'a> {
    code: RuleCode<'a>,
    level: String,
    message: String,
    uri: String,
    start_line: OneIndexed,
    start_column: OneIndexed,
    end_line: OneIndexed,
    end_column: OneIndexed,
}

impl<'a> SarifResult<'a> {
    #[cfg(not(target_arch = "wasm32"))]
    fn from_message(message: &'a Diagnostic) -> Result<Self> {
        let start_location = message.expect_ruff_start_location();
        let end_location = message.expect_ruff_end_location();
        let path = normalize_path(&*message.expect_ruff_filename());
        Ok(Self {
            code: RuleCode::from(message),
            level: "error".to_string(),
            message: message.body().to_string(),
            uri: url::Url::from_file_path(&path)
                .map_err(|()| anyhow::anyhow!("Failed to convert path to URL: {}", path.display()))?
                .to_string(),
            start_line: start_location.line,
            start_column: start_location.column,
            end_line: end_location.line,
            end_column: end_location.column,
        })
    }

    #[cfg(target_arch = "wasm32")]
    #[expect(clippy::unnecessary_wraps)]
    fn from_message(message: &'a Diagnostic) -> Result<Self> {
        let start_location = message.expect_ruff_start_location();
        let end_location = message.expect_ruff_end_location();
        let path = normalize_path(&*message.expect_ruff_filename());
        Ok(Self {
            code: RuleCode::from(message),
            level: "error".to_string(),
            message: message.body().to_string(),
            uri: path.display().to_string(),
            start_line: start_location.line,
            start_column: start_location.column,
            end_line: end_location.line,
            end_column: end_location.column,
        })
    }
}

impl Serialize for SarifResult<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        json!({
            "level": self.level,
            "message": {
                "text": self.message,
            },
            "locations": [{
                "physicalLocation": {
                    "artifactLocation": {
                        "uri": self.uri,
                    },
                    "region": {
                        "startLine": self.start_line,
                        "startColumn": self.start_column,
                        "endLine": self.end_line,
                        "endColumn": self.end_column,
                    }
                }
            }],
            "ruleId": self.code.as_str(),
        })
        .serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use crate::message::SarifEmitter;
    use crate::message::tests::{
        capture_emitter_output, create_diagnostics, create_syntax_error_diagnostics,
    };

    fn get_output() -> String {
        let mut emitter = SarifEmitter {};
        capture_emitter_output(&mut emitter, &create_diagnostics())
    }

    #[test]
    fn valid_json() {
        let content = get_output();
        serde_json::from_str::<serde_json::Value>(&content).unwrap();
    }

    #[test]
    fn valid_syntax_error_json() {
        let mut emitter = SarifEmitter {};
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_diagnostics());
        serde_json::from_str::<serde_json::Value>(&content).unwrap();
    }

    #[test]
    fn test_results() {
        let content = get_output();
        let value = serde_json::from_str::<serde_json::Value>(&content).unwrap();

        insta::assert_json_snapshot!(value, {
            ".runs[0].tool.driver.version" => "[VERSION]",
            ".runs[0].results[].locations[].physicalLocation.artifactLocation.uri" => "[URI]",
        });
    }
}
