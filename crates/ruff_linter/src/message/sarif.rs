use std::collections::HashSet;
use std::io::Write;

use anyhow::Result;
use serde::Serialize;

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

        let unique_rules: HashSet<_> = diagnostics
            .iter()
            .filter_map(Diagnostic::secondary_code)
            .collect();
        let mut rules: Vec<SarifRule> = unique_rules.into_iter().map(SarifRule::from).collect();
        rules.sort_by(|a, b| a.id.cmp(b.id));

        let output = SarifOutput {
            schema: "https://json.schemastore.org/sarif-2.1.0.json",
            version: "2.1.0",
            runs: [SarifRun {
                tool: SarifTool {
                    driver: SarifDriver {
                        name: "ruff",
                        information_uri: "https://github.com/astral-sh/ruff",
                        rules,
                        version: VERSION,
                    },
                },
                results,
            }],
        };
        serde_json::to_writer_pretty(writer, &output)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifRule<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    full_description: Option<MessageString<'a>>,
    help: MessageString<'a>,
    help_uri: Option<String>,
    id: &'a SecondaryCode,
    properties: SarifProperties<'a>,
    short_description: MessageString<'a>,
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
            id: code,
            help_uri: rule.url(),
            short_description: MessageString::from(rule.message_formats()[0]),
            full_description: rule.explanation().map(MessageString::from),
            help: MessageString::from(rule.message_formats()[0]),
            properties: SarifProperties {
                id: code,
                kind: linter.name(),
                name: rule.into(),
                problem_severity: "error",
            },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifResult<'a> {
    level: &'static str,
    locations: [SarifLocation; 1],
    message: MessageString<'a>,
    rule_id: &'a str,
}

impl<'a> SarifResult<'a> {
    #[cfg(not(target_arch = "wasm32"))]
    fn from_message(message: &'a Diagnostic) -> Result<Self> {
        let start_location = message.expect_ruff_start_location();
        let end_location = message.expect_ruff_end_location();
        let path = normalize_path(&*message.expect_ruff_filename());
        Ok(Self {
            rule_id: message
                .secondary_code()
                .map_or_else(|| message.name(), SecondaryCode::as_str),
            level: "error",
            message: MessageString::from(message.body()),
            locations: [SarifLocation {
                physical_location: SarifPhysicalLocation {
                    artifact_location: SarifArtifactLocation {
                        uri: url::Url::from_file_path(&path)
                            .map_err(|()| {
                                anyhow::anyhow!("Failed to convert path to URL: {}", path.display())
                            })?
                            .to_string(),
                    },
                    region: SarifRegion {
                        start_line: start_location.line,
                        start_column: start_location.column,
                        end_line: end_location.line,
                        end_column: end_location.column,
                    },
                },
            }],
        })
    }

    #[cfg(target_arch = "wasm32")]
    #[expect(clippy::unnecessary_wraps)]
    fn from_message(message: &'a Diagnostic) -> Result<Self> {
        let start_location = message.expect_ruff_start_location();
        let end_location = message.expect_ruff_end_location();
        let path = normalize_path(&*message.expect_ruff_filename());
        Ok(Self {
            rule_id: message
                .secondary_code()
                .map_or_else(|| message.name(), SecondaryCode::as_str),
            level: "error",
            message: MessageString::from(message.body()),
            locations: [SarifLocation {
                physical_location: SarifPhysicalLocation {
                    artifact_location: SarifArtifactLocation {
                        uri: path.display().to_string(),
                    },
                    region: SarifRegion {
                        start_line: start_location.line,
                        start_column: start_location.column,
                        end_line: end_location.line,
                        end_column: end_location.column,
                    },
                },
            }],
        })
    }
}

#[derive(Serialize)]
struct SarifOutput<'a> {
    #[serde(rename = "$schema")]
    schema: &'static str,
    runs: [SarifRun<'a>; 1],
    version: &'static str,
}

#[derive(Serialize)]
struct SarifRun<'a> {
    results: Vec<SarifResult<'a>>,
    tool: SarifTool<'a>,
}

#[derive(Serialize)]
struct SarifTool<'a> {
    driver: SarifDriver<'a>,
}

#[derive(Serialize)]
struct SarifDriver<'a> {
    #[serde(rename = "informationUri")]
    information_uri: &'static str,
    name: &'static str,
    rules: Vec<SarifRule<'a>>,
    version: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct SarifProperties<'a> {
    id: &'a SecondaryCode,
    kind: &'a str,
    name: &'a str,
    #[serde(rename = "problem.severity")]
    problem_severity: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct MessageString<'a> {
    text: &'a str,
}

impl<'a> From<&'a str> for MessageString<'a> {
    fn from(text: &'a str) -> Self {
        Self { text }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifLocation {
    physical_location: SarifPhysicalLocation,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifPhysicalLocation {
    artifact_location: SarifArtifactLocation,
    region: SarifRegion,
}

#[derive(Debug, Serialize)]
struct SarifArtifactLocation {
    uri: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifRegion {
    end_column: OneIndexed,
    end_line: OneIndexed,
    start_column: OneIndexed,
    start_line: OneIndexed,
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
