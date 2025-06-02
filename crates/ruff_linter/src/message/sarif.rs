use std::collections::HashSet;
use std::io::Write;

use anyhow::Result;
use serde::{Serialize, Serializer};
use serde_json::json;

use ruff_source_file::OneIndexed;

use crate::VERSION;
use crate::codes::Rule;
use crate::fs::normalize_path;
use crate::message::{Emitter, EmitterContext, Message};
use crate::registry::{Linter, RuleNamespace};

pub struct SarifEmitter;

impl Emitter for SarifEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        messages: &[Message],
        _context: &EmitterContext,
    ) -> Result<()> {
        let results = messages
            .iter()
            .map(SarifResult::from_message)
            .collect::<Result<Vec<_>>>()?;

        let unique_rules: HashSet<_> = results.iter().filter_map(|result| result.rule).collect();
        let mut rules: Vec<SarifRule> = unique_rules.into_iter().map(SarifRule::from).collect();
        rules.sort_by(|a, b| a.code.cmp(&b.code));

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
    code: String,
    linter: &'a str,
    summary: &'a str,
    explanation: Option<&'a str>,
    url: Option<String>,
}

impl From<Rule> for SarifRule<'_> {
    fn from(rule: Rule) -> Self {
        let code = rule.noqa_code().to_string();
        let (linter, _) = Linter::parse_code(&code).unwrap();
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
struct SarifResult {
    rule: Option<Rule>,
    level: String,
    message: String,
    uri: String,
    start_line: OneIndexed,
    start_column: OneIndexed,
    end_line: OneIndexed,
    end_column: OneIndexed,
}

impl SarifResult {
    #[cfg(not(target_arch = "wasm32"))]
    fn from_message(message: &Message) -> Result<Self> {
        let start_location = message.compute_start_location();
        let end_location = message.compute_end_location();
        let path = normalize_path(&*message.filename());
        Ok(Self {
            rule: message.to_rule(),
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
    fn from_message(message: &Message) -> Result<Self> {
        let start_location = message.compute_start_location();
        let end_location = message.compute_end_location();
        let path = normalize_path(&*message.filename());
        Ok(Self {
            rule: message.to_rule(),
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

impl Serialize for SarifResult {
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
            "ruleId": self.rule.map(|rule| rule.noqa_code().to_string()),
        })
        .serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use crate::message::SarifEmitter;
    use crate::message::tests::{
        capture_emitter_output, create_messages, create_syntax_error_messages,
    };

    fn get_output() -> String {
        let mut emitter = SarifEmitter {};
        capture_emitter_output(&mut emitter, &create_messages())
    }

    #[test]
    fn valid_json() {
        let content = get_output();
        serde_json::from_str::<serde_json::Value>(&content).unwrap();
    }

    #[test]
    fn valid_syntax_error_json() {
        let mut emitter = SarifEmitter {};
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_messages());
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
