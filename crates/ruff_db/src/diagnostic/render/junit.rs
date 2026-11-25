use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io::{BufWriter, Write as _};
use std::path::Path;

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesText, Event};

use ruff_source_file::LineColumn;

use crate::diagnostic::{
    Diagnostic, DisplayDiagnosticConfig, Severity,
    render::{FileResolver, FmtAdapter, Renderer},
};

/// Print diagnostics as a JUnit-style XML report.
///
/// See [`junit.xsd`] for the specification in the JUnit repository and an annotated [version].
///
/// [`junit.xsd`]: https://github.com/junit-team/junit-framework/blob/2870b7d8fd5bf7c1efe489d3991d3ed3900e82bb/platform-tests/src/test/resources/jenkins-junit.xsd
/// [version]: https://llg.cubic.org/docs/junit/
pub struct JunitRenderer<'a> {
    resolver: &'a dyn FileResolver,
    config: &'a DisplayDiagnosticConfig,
}

impl<'a> JunitRenderer<'a> {
    pub fn new(resolver: &'a dyn FileResolver, config: &'a DisplayDiagnosticConfig) -> Self {
        Self { resolver, config }
    }

    fn write_report(
        &self,
        output: impl std::io::Write,
        diagnostics: &[Diagnostic],
    ) -> std::io::Result<()> {
        let mut writer = Writer::new_with_indent(output, b' ', 4);
        writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;
        let program = self.config.program;
        let package = format!("org.{program}");
        writer
            .create_element("testsuites")
            .with_attribute(("name", xml_string(program).as_str()))
            .with_attribute(("tests", diagnostics.len().max(1).to_string().as_str()))
            .with_attribute(("skipped", "0"))
            .with_attribute(("failures", diagnostics.len().to_string().as_str()))
            .with_attribute(("errors", "0"))
            .write_inner_content(|writer| {
                if diagnostics.is_empty() {
                    self.write_suite(writer, program, &package, &[])?;
                } else {
                    for (filename, diagnostics) in
                        group_diagnostics_by_filename(diagnostics, self.resolver)
                    {
                        self.write_suite(writer, filename, &package, &diagnostics)?;
                    }
                }
                Ok(())
            })?;
        writer.write_indent()
    }

    fn write_suite<W: std::io::Write>(
        &self,
        writer: &mut Writer<W>,
        filename: &str,
        package: &str,
        diagnostics: &[DiagnosticWithLocation<'_>],
    ) -> std::io::Result<()> {
        writer
            .create_element("testsuite")
            .with_attribute(("name", xml_string(filename).as_str()))
            .with_attribute(("tests", diagnostics.len().max(1).to_string().as_str()))
            .with_attribute(("skipped", "0"))
            .with_attribute(("errors", "0"))
            .with_attribute(("failures", diagnostics.len().to_string().as_str()))
            .with_attribute(("package", xml_string(package).as_str()))
            .write_inner_content(|writer| {
                if diagnostics.is_empty() {
                    writer
                        .create_element("testcase")
                        .with_attribute(("name", "No errors found"))
                        .with_attribute(("classname", xml_string(filename).as_str()))
                        .write_empty()?;
                    return Ok(());
                }

                let classname = Path::new(filename).with_extension("");
                let classname = classname.to_str().unwrap_or(filename);
                for DiagnosticWithLocation {
                    diagnostic,
                    start_location: location,
                } in diagnostics
                {
                    let code = if self.config.preview && !self.config.prefer_rule_codes {
                        diagnostic.id().as_str()
                    } else {
                        diagnostic.secondary_code_or_id()
                    };
                    let name = format!("org.{}.{code}", self.config.program);
                    write_failure_case(
                        writer,
                        FailureCase {
                            name: &name,
                            classname,
                            location: *location,
                            severity: diagnostic.severity(),
                            message: &diagnostic.concise_message().to_str(),
                        },
                    )?;
                }
                Ok(())
            })?;
        Ok(())
    }
}

impl Renderer for JunitRenderer<'_> {
    fn render(&self, f: &mut std::fmt::Formatter, diagnostics: &[Diagnostic]) -> std::fmt::Result {
        let mut writer = BufWriter::new(FmtAdapter { fmt: f });
        self.write_report(&mut writer, diagnostics)
            .map_err(|_| std::fmt::Error)?;
        writer.flush().map_err(|_| std::fmt::Error)
    }
}

struct FailureCase<'a> {
    name: &'a str,
    classname: &'a str,
    location: Option<LineColumn>,
    severity: Severity,
    message: &'a str,
}

fn write_failure_case<W: std::io::Write>(
    writer: &mut Writer<W>,
    case: FailureCase<'_>,
) -> std::io::Result<()> {
    let mut testcase = writer
        .create_element("testcase")
        .with_attribute(("name", xml_string(case.name).as_str()))
        .with_attribute(("classname", xml_string(case.classname).as_str()));
    if let Some(location) = case.location {
        testcase = testcase
            .with_attribute(("line", location.line.to_string().as_str()))
            .with_attribute(("column", location.column.to_string().as_str()));
    }
    testcase.write_inner_content(|writer| {
        writer
            .create_element("properties")
            .write_inner_content(|writer| {
                writer
                    .create_element("property")
                    .with_attribute(("name", "severity"))
                    .with_attribute(("value", severity_name(case.severity)))
                    .write_empty()?;
                Ok(())
            })?;
        let message = xml_string(case.message);
        let description = if let Some(location) = case.location {
            Cow::Owned(format!(
                "line {}, col {}, {message}",
                location.line, location.column
            ))
        } else {
            Cow::Borrowed(message.as_str())
        };
        writer
            .create_element("failure")
            .with_attribute(("message", message.as_str()))
            .write_text_content(BytesText::new(&description))?;
        Ok(())
    })?;
    Ok(())
}

/// Preserve quick-junit's string sanitization. Filenames and diagnostic messages can contain
/// terminal escapes; XML 1.0 forbids ESC, and removing the entire sequence avoids stray text.
fn xml_string(value: &str) -> String {
    strip_ansi_escapes::strip_str(value).replace(
        |c| matches!(c, '\x00'..='\x08' | '\x0b' | '\x0c' | '\x0e'..='\x1f'),
        "",
    )
}

const fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Hint => "hint",
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "error",
        Severity::Fatal => "fatal",
    }
}

struct DiagnosticWithLocation<'a> {
    diagnostic: &'a Diagnostic,
    start_location: Option<LineColumn>,
}

fn group_diagnostics_by_filename<'a>(
    diagnostics: &'a [Diagnostic],
    resolver: &'a dyn FileResolver,
) -> BTreeMap<&'a str, Vec<DiagnosticWithLocation<'a>>> {
    let mut grouped_diagnostics = BTreeMap::default();
    for diagnostic in diagnostics {
        let (filename, start_location) = diagnostic.primary_span_ref().map_or_default(|span| {
            let file = span.file();
            let start_location =
                span.range()
                    .filter(|_| !resolver.is_notebook(file))
                    .map(|range| {
                        file.diagnostic_source(resolver)
                            .as_source_code()
                            .line_column(range.start())
                    });

            (span.file().path(resolver), start_location)
        });

        grouped_diagnostics
            .entry(filename)
            .or_insert_with(Vec::new)
            .push(DiagnosticWithLocation {
                diagnostic,
                start_location,
            });
    }
    grouped_diagnostics
}

#[cfg(test)]
mod tests {
    use super::xml_string;
    use crate::diagnostic::render::tests::{
        DiagnosticFormat, create_diagnostics, create_syntax_error_diagnostics,
    };

    #[test]
    fn output() {
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Junit);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Junit);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn empty_report() {
        let (env, _) = create_diagnostics(DiagnosticFormat::Junit);
        let report = env.render_diagnostics(&[]);
        assert!(report.contains("tests=\"1\" skipped=\"0\" failures=\"0\" errors=\"0\""));
        assert!(report.contains("<testcase name=\"No errors found\" classname=\"ty\"/>"));
    }

    #[test]
    fn sanitizes_and_escapes_diagnostic_messages() {
        let (env, mut diagnostics) = create_diagnostics(DiagnosticFormat::Junit);
        diagnostics[0].set_concise_message("\x1b[31mred\x1b[0m & <\0");
        let report = env.render_diagnostics(&diagnostics[..1]);
        assert!(report.contains("message=\"red &amp; &lt;\""));
        assert!(report.contains("red &amp; &lt;</failure>"));
        assert!(!report.contains(['\x1b', '\0']));
    }

    #[test]
    fn strips_terminal_escapes_and_xml_control_characters() {
        assert_eq!(xml_string("\x1b[31mred\x1b[0m\0\t\n"), "red\n");
    }
}
