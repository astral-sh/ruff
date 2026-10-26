use std::borrow::Cow;
use std::collections::HashMap;

use rustc_hash::FxHashMap;
use serde::{Serialize, Serializer};

use ruff_source_file::{OneIndexed, SourceCode};
use ruff_text_size::{Ranged, TextRange};

use crate::diagnostic::{
    Diagnostic, DisplayDiagnosticConfig, LintName, RuleMetadata, SecondaryCode, Severity,
};

use super::{FileResolver, Renderer};

/// A renderer for producing SARIF 2.1.0-compliant JSON output.
///
/// Static Analysis Results Interchange Format (SARIF) is a standard format
/// for static analysis results. For full specification, see:
/// [SARIF 2.1.0](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html)
pub struct SarifRenderer<'a> {
    resolver: &'a dyn FileResolver,
    config: &'a DisplayDiagnosticConfig,
    metadata: FxHashMap<LintName, RuleMetadata>,
    version: &'a str,
}

impl<'a> SarifRenderer<'a> {
    pub fn new(
        resolver: &'a dyn FileResolver,
        config: &'a DisplayDiagnosticConfig,
        metadata: FxHashMap<LintName, RuleMetadata>,
        version: &'a str,
    ) -> Self {
        Self {
            resolver,
            config,
            metadata,
            version,
        }
    }
}

impl Renderer for SarifRenderer<'_> {
    fn render(&self, f: &mut std::fmt::Formatter, diagnostics: &[Diagnostic]) -> std::fmt::Result {
        let results = diagnostics
            .iter()
            .map(|diagnostic| SarifResult::from_message(diagnostic, self.resolver, self.config))
            .collect::<Vec<_>>();

        let mut unique_rules = HashMap::new();
        for result in &results {
            if let Some(name) = result.lint_name {
                // A rule's severity can differ between files. Keep one descriptor per rule,
                // using its highest reported severity for GitHub's `problem.severity`;
                // each result retains its own level.
                unique_rules
                    .entry((name, result.rule_id.as_str()))
                    .and_modify(|level: &mut SarifLevel| *level = (*level).max(result.level))
                    .or_insert(result.level);
            }
        }
        let mut rules: Vec<SarifRule> = unique_rules
            .into_iter()
            .filter_map(|((name, code), level)| {
                let metadata = self.metadata.get(&name)?;
                Some(SarifRule::new(code, level, metadata))
            })
            .collect();
        rules.sort_by(|a, b| {
            a.properties
                .problem_severity
                .cmp(&b.properties.problem_severity)
                .then_with(|| a.id.cmp(b.id))
        });
        let output = SarifOutput {
            schema: "https://json.schemastore.org/sarif-2.1.0.json",
            version: "2.1.0",
            runs: [SarifRun {
                tool: SarifTool {
                    driver: SarifDriver {
                        name: self.config.program,
                        information_uri: format!(
                            "https://github.com/astral-sh/{}",
                            self.config.program,
                        ),
                        rules,
                        version: self.version,
                    },
                },
                results: &results,
            }],
        };
        write!(f, "{:#}", serde_json::json!(output))
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
    results: &'a [SarifResult<'a>],
    tool: SarifTool<'a>,
}

#[derive(Serialize)]
struct SarifTool<'a> {
    driver: SarifDriver<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifDriver<'a> {
    information_uri: String,
    name: &'static str,
    rules: Vec<SarifRule<'a>>,
    version: &'a str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifRule<'a> {
    default_configuration: SarifReportingConfiguration,
    full_description: SarifMessage<'a>,
    help: SarifMessage<'a>,
    help_uri: &'a str,
    id: &'a str,
    properties: SarifProperties<'a>,
    short_description: SarifMessage<'a>,
}

impl<'a> SarifRule<'a> {
    fn new(code: &'a str, level: SarifLevel, metadata: &'a RuleMetadata) -> Self {
        Self {
            id: code,
            default_configuration: SarifReportingConfiguration {
                enabled: metadata.enabled_by_default,
                level: metadata.default_severity.map(SarifLevel::from),
            },
            short_description: SarifMessage {
                text: Cow::Borrowed(metadata.summary),
            },
            full_description: SarifMessage {
                text: Cow::Borrowed(&metadata.description),
            },
            help: SarifMessage {
                text: Cow::Borrowed(metadata.summary),
            },
            help_uri: &metadata.help_url,
            properties: SarifProperties {
                id: code,
                // Preserve the existing SARIF output when preview prefers rule names:
                // omit the linter even if the rule also has a code.
                kind: if code == metadata.name.as_str() {
                    None
                } else {
                    metadata.linter
                },
                name: metadata.name.as_str(),
                problem_severity: level,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct SarifReportingConfiguration {
    enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    level: Option<SarifLevel>,
}

#[derive(Debug)]
enum RuleCode<'a> {
    SecondaryCode(&'a SecondaryCode),
    LintId(&'a str),
}

impl RuleCode<'_> {
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

impl<'a> RuleCode<'a> {
    fn from_diagnostic(code: &'a Diagnostic, config: &'a DisplayDiagnosticConfig) -> Self {
        match code.secondary_code() {
            Some(diagnostic)
                if !config.preview_enabled() || config.is_prefer_rule_codes_enabled() =>
            {
                Self::SecondaryCode(diagnostic)
            }
            _ => Self::LintId(code.id().as_str()),
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    fixes: Vec<SarifFix>,
    level: SarifLevel,
    locations: Vec<SarifLocation>,
    message: SarifMessage<'a>,
    rule_id: RuleCode<'a>,
    #[serde(skip)]
    lint_name: Option<LintName>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifMessage<'a> {
    text: Cow<'a, str>,
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
    artifact_changes: Vec<SarifArtifactChange>,
    description: RuleDescription,
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

#[derive(Debug, Default, Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
struct SarifRegion {
    end_column: OneIndexed,
    end_line: OneIndexed,
    start_column: OneIndexed,
    start_line: OneIndexed,
}

#[derive(Debug, Clone, Serialize)]
struct SarifProperties<'a> {
    id: &'a str,
    kind: Option<&'a str>,
    name: &'a str,
    #[serde(rename = "problem.severity")]
    problem_severity: SarifLevel,
}

#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
enum SarifLevel {
    Note,
    Warning,
    Error,
}

impl From<Severity> for SarifLevel {
    fn from(value: Severity) -> Self {
        match value {
            Severity::Info => SarifLevel::Note,
            Severity::Warning => SarifLevel::Warning,
            Severity::Error | Severity::Fatal => SarifLevel::Error,
        }
    }
}

impl<'a> SarifResult<'a> {
    fn range_to_sarif_region(source_code: &SourceCode, range: TextRange) -> SarifRegion {
        let start_location = source_code.line_column(range.start());
        let end_location = source_code.line_column(range.end());

        SarifRegion {
            start_line: start_location.line,
            start_column: start_location.column,
            end_line: end_location.line,
            end_column: end_location.column,
        }
    }

    fn fix(diagnostic: &'a Diagnostic, uri: &str, source_code: &SourceCode) -> Option<SarifFix> {
        let fix = diagnostic.fix()?;

        let fix_description = diagnostic
            .first_help_text()
            .map(std::string::ToString::to_string);

        let replacements: Vec<SarifReplacement> = fix
            .edits()
            .iter()
            .map(|edit| {
                let range = edit.range();
                let deleted_region = Self::range_to_sarif_region(source_code, range);
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

    fn uri(path: &str) -> String {
        cfg_select! {
            target_arch = "wasm32" => format!("file://{path}"),
            _ => url::Url::from_file_path(path)
                .map_or_else(|()| format!("file://{path}"), |url| url.to_string()),
        }
    }

    fn from_message(
        diagnostic: &'a Diagnostic,
        resolver: &dyn FileResolver,
        config: &'a DisplayDiagnosticConfig,
    ) -> Self {
        let (locations, fixes) = diagnostic
            .primary_span()
            .map(|span| {
                let file = span.file();
                let uri = Self::uri(file.path(resolver));
                let source = file.diagnostic_source(resolver);
                let source_code = source.as_source_code();
                let region = span
                    .range()
                    .map(|range| Self::range_to_sarif_region(&source_code, range))
                    .unwrap_or_default();
                let fixes = Self::fix(diagnostic, &uri, &source_code)
                    .into_iter()
                    .collect();
                let locations = vec![SarifLocation {
                    physical_location: SarifPhysicalLocation {
                        artifact_location: SarifArtifactLocation { uri },
                        region,
                    },
                }];
                (locations, fixes)
            })
            .unwrap_or_default();

        Self {
            rule_id: RuleCode::from_diagnostic(diagnostic, config),
            level: SarifLevel::from(diagnostic.severity()),
            message: SarifMessage {
                text: diagnostic.concise_message().to_str(),
            },
            fixes,
            locations,
            lint_name: diagnostic.id().as_lint(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::render::tests::{
        DiagnosticFormat, create_diagnostics, create_syntax_error_diagnostics,
    };

    fn get_output() -> String {
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Sarif);
        env.render_diagnostics(&diagnostics)
    }

    #[test]
    fn valid_json() {
        let content = get_output();
        serde_json::from_str::<serde_json::Value>(&content).unwrap();
    }

    #[test]
    fn valid_syntax_error_json() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Sarif);
        let content = env.render_diagnostics(&diagnostics);
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
