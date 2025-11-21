use std::collections::HashSet;
use std::io::Write;

use anyhow::Result;
use log::warn;
use serde::{Serialize, Serializer};
use serde_json::json;

use ruff_db::diagnostic::{Diagnostic, SecondaryCode};
use ruff_source_file::{OneIndexed, SourceFile};
use ruff_text_size::{Ranged, TextRange};

use crate::VERSION;
use crate::fs::normalize_path;
use crate::message::{Emitter, EmitterContext};
use crate::registry::{Linter, RuleNamespace};

/// An emitter for producing SARIF 2.1.0-compliant JSON output.
///
/// Static Analysis Results Interchange Format (SARIF) is a standard format
/// for static analysis results. For full specfification, see:
/// [SARIF 2.1.0](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html)
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
            .filter_map(|result| result.rule_id.as_secondary_code())
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

impl Serialize for RuleCode<'_> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
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

/// Represents a single result in a SARIF 2.1.0 report.
///
/// See the SARIF 2.1.0 specification for details:
/// [SARIF 2.1.0](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifResult<'a> {
    rule_id: RuleCode<'a>,
    level: String,
    message: SarifMessage,
    locations: Vec<SarifLocation>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    fixes: Vec<SarifFix>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifMessage {
    text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifPhysicalLocation {
    artifact_location: SarifArtifactLocation,
    region: SarifRegion,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifLocation {
    physical_location: SarifPhysicalLocation,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifFix {
    description: RuleDescription,
    artifact_changes: Vec<SarifArtifactChange>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuleDescription {
    text: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifArtifactChange {
    artifact_location: SarifArtifactLocation,
    replacements: Vec<SarifReplacement>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifArtifactLocation {
    uri: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifReplacement {
    deleted_region: SarifRegion,
    #[serde(skip_serializing_if = "Option::is_none")]
    inserted_content: Option<InsertedContent>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InsertedContent {
    text: String,
}

#[derive(Debug, Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
struct SarifRegion {
    start_line: OneIndexed,
    start_column: OneIndexed,
    end_line: OneIndexed,
    end_column: OneIndexed,
}

impl<'a> SarifResult<'a> {
    fn range_to_sarif_region(source_file: &SourceFile, range: TextRange) -> SarifRegion {
        let source_code = source_file.to_source_code();
        let start_location = source_code.line_column(range.start());
        let end_location = source_code.line_column(range.end());

        SarifRegion {
            start_line: start_location.line,
            start_column: start_location.column,
            end_line: end_location.line,
            end_column: end_location.column,
        }
    }

    fn fix(diagnostic: &'a Diagnostic, uri: &str) -> Option<SarifFix> {
        let fix = diagnostic.fix()?;

        let Some(source_file) = diagnostic.ruff_source_file() else {
            debug_assert!(
                false,
                "Omitting the fix for diagnostic with id `{}` because the source file is missing. This is a bug in Ruff, please report an issue.",
                diagnostic.id()
            );

            warn!(
                "Omitting the fix for diagnostic with id `{}` because the source file is missing. This is a bug in Ruff, please report an issue.",
                diagnostic.id()
            );
            return None;
        };

        let fix_description = diagnostic
            .first_help_text()
            .map(std::string::ToString::to_string);

        let replacements: Vec<SarifReplacement> = fix
            .edits()
            .iter()
            .map(|edit| {
                let range = edit.range();
                let deleted_region = Self::range_to_sarif_region(source_file, range);
                SarifReplacement {
                    deleted_region,
                    inserted_content: edit.content().map(|content| InsertedContent {
                        text: content.to_string(),
                    }),
                }
            })
            .collect();

        let artifact_changes = vec![SarifArtifactChange {
            artifact_location: SarifArtifactLocation {
                uri: uri.to_string(),
            },
            replacements,
        }];

        Some(SarifFix {
            description: RuleDescription {
                text: fix_description,
            },
            artifact_changes,
        })
    }

    #[allow(clippy::unnecessary_wraps)]
    fn uri(diagnostic: &Diagnostic) -> Result<String> {
        let path = normalize_path(&*diagnostic.expect_ruff_filename());
        #[cfg(not(target_arch = "wasm32"))]
        return url::Url::from_file_path(&path)
            .map_err(|()| anyhow::anyhow!("Failed to convert path to URL: {}", path.display()))
            .map(|u| u.to_string());
        #[cfg(target_arch = "wasm32")]
        return Ok(format!("file://{}", path.display()));
    }

    fn from_message(diagnostic: &'a Diagnostic) -> Result<Self> {
        let start_location = diagnostic.ruff_start_location().unwrap_or_default();
        let end_location = diagnostic.ruff_end_location().unwrap_or_default();
        let region = SarifRegion {
            start_line: start_location.line,
            start_column: start_location.column,
            end_line: end_location.line,
            end_column: end_location.column,
        };

        let uri = Self::uri(diagnostic)?;

        Ok(Self {
            rule_id: RuleCode::from(diagnostic),
            level: "error".to_string(),
            message: SarifMessage {
                text: diagnostic.body().to_string(),
            },
            fixes: Self::fix(diagnostic, &uri).into_iter().collect(),
            locations: vec![SarifLocation {
                physical_location: SarifPhysicalLocation {
                    artifact_location: SarifArtifactLocation { uri },
                    region,
                },
            }],
        })
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
            ".runs[0].results[].fixes[].artifactChanges[].artifactLocation.uri" => "[URI]",
        });
    }
}
