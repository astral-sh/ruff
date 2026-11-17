use std::collections::BTreeMap;
use std::io::{BufWriter, Write as _};
use std::path::Path;

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};

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
        let mut xml = JunitWriter::new(output);
        let program = self.config.program;
        let package = format!("org.{program}");
        xml.start_report(program, diagnostics.len())?;

        if diagnostics.is_empty() {
            xml.start_suite(program, 1, 0, &package)?;
            xml.write_success_case("No errors found", program)?;
            xml.end_suite()?;
        } else {
            for (filename, diagnostics) in group_diagnostics_by_filename(diagnostics, self.resolver)
            {
                let count = diagnostics.len();
                xml.start_suite(filename, count, count, &package)?;
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
                    let name = format!("org.{program}.{code}");
                    xml.write_failure_case(FailureCase {
                        name: &name,
                        classname,
                        location,
                        severity: diagnostic.severity(),
                        message: &diagnostic.concise_message().to_str(),
                    })?;
                }
                xml.end_suite()?;
            }
        }
        xml.finish()
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

struct JunitWriter<W> {
    writer: Writer<W>,
}

impl<W: std::io::Write> JunitWriter<W> {
    fn new(output: W) -> Self {
        Self {
            writer: Writer::new_with_indent(output, b' ', 4),
        }
    }

    fn start_report(&mut self, program: &str, failures: usize) -> std::io::Result<()> {
        self.writer
            .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;
        self.start(
            XmlElement::new("testsuites")
                .attr("name", program)
                .attr("tests", &failures.max(1).to_string())
                .attr("skipped", "0")
                .attr("failures", &failures.to_string())
                .attr("errors", "0"),
        )
    }

    fn start_suite(
        &mut self,
        name: &str,
        tests: usize,
        failures: usize,
        package: &str,
    ) -> std::io::Result<()> {
        self.start(
            XmlElement::new("testsuite")
                .attr("name", name)
                .attr("tests", &tests.to_string())
                .attr("skipped", "0")
                .attr("errors", "0")
                .attr("failures", &failures.to_string())
                .attr("package", package),
        )
    }

    fn write_success_case(&mut self, name: &str, classname: &str) -> std::io::Result<()> {
        self.empty(
            XmlElement::new("testcase")
                .attr("name", name)
                .attr("classname", classname),
        )
    }

    fn write_failure_case(&mut self, case: FailureCase<'_>) -> std::io::Result<()> {
        let mut testcase = XmlElement::new("testcase")
            .attr("name", case.name)
            .attr("classname", case.classname);
        if let Some(location) = case.location {
            testcase = testcase
                .attr("line", &location.line.to_string())
                .attr("column", &location.column.to_string());
        }
        self.start(testcase)?;
        self.start(XmlElement::new("properties"))?;
        self.empty(
            XmlElement::new("property")
                .attr("name", "severity")
                .attr("value", severity_name(case.severity)),
        )?;
        self.end("properties")?;
        self.start(XmlElement::new("failure").attr("message", case.message))?;
        if let Some(location) = case.location {
            self.text(&format!(
                "line {}, col {}, {}",
                location.line, location.column, case.message
            ))?;
        } else {
            self.text(case.message)?;
        }
        self.end("failure")?;
        self.end("testcase")
    }

    fn end_suite(&mut self) -> std::io::Result<()> {
        self.end("testsuite")
    }

    fn finish(mut self) -> std::io::Result<()> {
        self.end("testsuites")?;
        self.writer.write_indent()
    }

    fn start(&mut self, element: XmlElement) -> std::io::Result<()> {
        self.writer.write_event(Event::Start(element.0))
    }

    fn empty(&mut self, element: XmlElement) -> std::io::Result<()> {
        self.writer.write_event(Event::Empty(element.0))
    }

    fn text(&mut self, value: &str) -> std::io::Result<()> {
        let value = xml_string(value);
        self.writer.write_event(Event::Text(BytesText::new(&value)))
    }

    fn end(&mut self, name: &str) -> std::io::Result<()> {
        self.writer.write_event(Event::End(BytesEnd::new(name)))
    }
}

/// An XML element whose attribute values are sanitized before quick-xml escapes them.
struct XmlElement(BytesStart<'static>);

impl XmlElement {
    fn new(name: &'static str) -> Self {
        Self(BytesStart::new(name))
    }

    fn attr(mut self, key: &str, value: &str) -> Self {
        let value = xml_string(value);
        self.0.push_attribute((key, value.as_str()));
        self
    }
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
    fn strips_terminal_escapes_and_xml_control_characters() {
        assert_eq!(xml_string("\x1b[31mred\x1b[0m\0\t\n"), "red\n");
    }
}
