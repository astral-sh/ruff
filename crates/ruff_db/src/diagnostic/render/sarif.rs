use super::FileResolver;
use crate::diagnostic::{Diagnostic, SecondaryCode, Severity};
use ruff_source_file::OneIndexed;
use ruff_text_size::Ranged;
use serde::{Serialize, Serializer};
use serde_json::json;
use std::collections::HashMap;

pub struct SarifToolInfo {
    pub name: &'static str,
    pub information_uri: &'static str,
}

pub struct SarifRenderer<'a> {
    resolver: &'a dyn FileResolver,
    tool: SarifToolInfo,
}

impl<'a> SarifRenderer<'a> {
    pub fn new(resolver: &'a dyn FileResolver, tool: SarifToolInfo) -> Self {
        Self { resolver, tool }
    }
}

impl SarifRenderer<'_> {
    pub(super) fn render(
        &self,
        f: &mut std::fmt::Formatter,
        diagnostics: &[Diagnostic],
    ) -> std::fmt::Result {
        let results = diagnostics
            .iter()
            .map(|diagnostic| SarifResult::from_diagnostic(diagnostic, self.resolver))
            .collect::<Vec<_>>();

        let unique_rules: HashMap<&SecondaryCode, SarifRuleInfo> = diagnostics
            .iter()
            .filter_map(|d| {
                d.secondary_code().map(|code| {
                    (
                        code,
                        SarifRuleInfo {
                            code,
                            message: d.primary_message(),
                            url: d.documentation_url(),
                        },
                    )
                })
            })
            .collect();

        let mut rules: Vec<SarifRule> = unique_rules.into_values().map(SarifRule::from).collect();
        rules.sort_by(|a, b| a.info.code.cmp(b.info.code));

        let output = serde_json::json!({
            "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
            "version": "2.1.0",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": self.tool.name,
                        "informationUri": self.tool.information_uri,
                        "rules": rules,
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                },
                "results": results,
            }],
        });

        write!(
            f,
            "{}",
            serde_json::to_string_pretty(&output).map_err(|_| std::fmt::Error)?
        )
    }
}

#[derive(Debug)]
enum RuleCode {
    SecondaryCode(String),
    Other(String),
}

impl RuleCode {
    fn as_str(&self) -> &str {
        match self {
            RuleCode::SecondaryCode(code) => code.as_str(),
            RuleCode::Other(s) => s.as_str(),
        }
    }
}

impl Serialize for RuleCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl From<&Diagnostic> for RuleCode {
    fn from(diagnostic: &Diagnostic) -> Self {
        match diagnostic.secondary_code() {
            Some(code) => Self::SecondaryCode(code.as_str().to_string()),
            None => Self::Other(diagnostic.secondary_code_or_id().to_string()),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifResult {
    rule_id: RuleCode,
    level: String,
    message: SarifMessage,
    locations: Vec<SarifLocation>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    fixes: Vec<SarifFix>,
}

impl SarifResult {
    fn from_diagnostic(diagnostic: &Diagnostic, resolver: &dyn FileResolver) -> Self {
        let (location, uri) = diagnostic
            .primary_span()
            .map(|span| {
                let file = span.file();
                let uri = format!("file://{}", file.relative_path(resolver).display());

                let region = if resolver.is_notebook(file) {
                    SarifRegion::default()
                } else {
                    let diagnostic_source = file.diagnostic_source(resolver);
                    let source_code = diagnostic_source.as_source_code();

                    span.range()
                        .map(|range| {
                            let start = source_code.line_column(range.start());
                            let end = source_code.line_column(range.end());
                            SarifRegion {
                                start_line: start.line,
                                start_column: start.column,
                                end_line: end.line,
                                end_column: end.column,
                            }
                        })
                        .unwrap_or_default()
                };

                let location = SarifLocation {
                    physical_location: SarifPhysicalLocation {
                        artifact_location: SarifArtifactLocation { uri: uri.clone() },
                        region,
                    },
                };

                (location, uri)
            })
            .unwrap_or_default();

        let level = match diagnostic.severity() {
            Severity::Info => "note",
            Severity::Warning => "warning",
            Severity::Error => "error",
            Severity::Fatal => "error",
        };

        Self {
            rule_id: RuleCode::from(diagnostic),
            level: level.to_string(),
            message: SarifMessage {
                text: diagnostic.concise_message().to_string(),
            },
            locations: vec![location],
            fixes: Self::extract_fixes(diagnostic, &uri, resolver),
        }
    }

    fn extract_fixes(
        diagnostic: &Diagnostic,
        uri: &str,
        resolver: &dyn FileResolver,
    ) -> Vec<SarifFix> {
        let Some(fix) = diagnostic.fix() else {
            return vec![];
        };

        let Some(span) = diagnostic.primary_span() else {
            return vec![];
        };

        let file = span.file();

        if resolver.is_notebook(file) {
            return vec![];
        }

        let diagnostic_source = file.diagnostic_source(resolver);
        let source_code = diagnostic_source.as_source_code();

        let fix_description = diagnostic
            .first_help_text()
            .map(std::string::ToString::to_string);

        let replacements: Vec<SarifReplacement> = fix
            .edits()
            .iter()
            .map(|edit| {
                let range = edit.range();
                let start = source_code.line_column(range.start());
                let end = source_code.line_column(range.end());

                SarifReplacement {
                    deleted_region: SarifRegion {
                        start_line: start.line,
                        start_column: start.column,
                        end_line: end.line,
                        end_column: end.column,
                    },
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

        vec![SarifFix {
            description: RuleDescription {
                text: fix_description,
            },
            artifact_changes,
        }]
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifMessage {
    text: String,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct SarifPhysicalLocation {
    artifact_location: SarifArtifactLocation,
    region: SarifRegion,
}

#[derive(Debug, Serialize, Default)]
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

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct SarifArtifactChange {
    artifact_location: SarifArtifactLocation,
    replacements: Vec<SarifReplacement>,
}

#[derive(Debug, Serialize, Default)]
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

#[derive(Debug, Serialize, Clone, Copy, Default)]
#[serde(rename_all = "camelCase")]
struct SarifRegion {
    start_line: OneIndexed,
    start_column: OneIndexed,
    end_line: OneIndexed,
    end_column: OneIndexed,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
struct SarifRuleInfo<'a> {
    code: &'a SecondaryCode,
    message: &'a str,
    url: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SarifRule<'a> {
    info: SarifRuleInfo<'a>,
}

impl<'a> From<SarifRuleInfo<'a>> for SarifRule<'a> {
    fn from(info: SarifRuleInfo<'a>) -> Self {
        Self { info }
    }
}

impl Serialize for SarifRule<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut rule = json!({
            "id": self.info.code,
            "shortDescription": {
                "text": self.info.message,
            },
            "properties": {
                "id": self.info.code,
            },
        });

        if let Some(url) = self.info.url {
            rule["helpUri"] = json!(url);
        }

        rule.serialize(serializer)
    }
}

pub struct DisplaySarifDiagnostics<'a> {
    renderer: &'a SarifRenderer<'a>,
    diagnostics: &'a [Diagnostic],
}

impl<'a> DisplaySarifDiagnostics<'a> {
    pub fn new(renderer: &'a SarifRenderer<'a>, diagnostics: &'a [Diagnostic]) -> Self {
        Self {
            renderer,
            diagnostics,
        }
    }
}

impl std::fmt::Display for DisplaySarifDiagnostics<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.renderer.render(f, self.diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::{
        DiagnosticFormat,
        render::tests::{create_diagnostics, create_syntax_error_diagnostics},
    };

    #[test]
    fn output() {
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Sarif);
        let output = env.render_diagnostics(&diagnostics);
        serde_json::from_str::<serde_json::Value>(&output).unwrap();
        insta::assert_snapshot!(output);
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Sarif);
        let output = env.render_diagnostics(&diagnostics);
        serde_json::from_str::<serde_json::Value>(&output).unwrap();
        insta::assert_snapshot!(output);
    }
}
