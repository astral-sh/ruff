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
        xml.writer
            .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;

        let program = self.config.program;
        let package = format!("org.{program}");
        let tests = diagnostics.len().max(1).to_string();
        let failures = diagnostics.len().to_string();
        xml.start(
            "testsuites",
            &[
                ("name", program),
                ("tests", &tests),
                ("skipped", "0"),
                ("failures", &failures),
                ("errors", "0"),
            ],
        )?;

        if diagnostics.is_empty() {
            xml.start_suite(program, "1", "0", &package)?;
            xml.empty(
                "testcase",
                &[("name", "No errors found"), ("classname", program)],
            )?;
            xml.end("testsuite")?;
        } else {
            for (filename, diagnostics) in group_diagnostics_by_filename(diagnostics, self.resolver)
            {
                let count = diagnostics.len().to_string();
                xml.start_suite(filename, &count, &count, &package)?;
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
                    let mut attributes = vec![("name", name.as_str()), ("classname", classname)];
                    let row;
                    let column;
                    if let Some(location) = location {
                        row = location.line.to_string();
                        column = location.column.to_string();
                        attributes.extend([("line", row.as_str()), ("column", column.as_str())]);
                    }
                    xml.start("testcase", &attributes)?;
                    xml.start("properties", &[])?;
                    xml.empty(
                        "property",
                        &[
                            ("name", "severity"),
                            ("value", severity_name(diagnostic.severity())),
                        ],
                    )?;
                    xml.end("properties")?;

                    let message = diagnostic.concise_message();
                    xml.start("failure", &[("message", &message.to_str())])?;
                    let description = if let Some(location) = location {
                        format!(
                            "line {row}, col {col}, {message}",
                            row = location.line,
                            col = location.column
                        )
                    } else {
                        message.to_string()
                    };
                    xml.text(&description)?;
                    xml.end("failure")?;
                    xml.end("testcase")?;
                }
                xml.end("testsuite")?;
            }
        }
        xml.end("testsuites")?;
        xml.writer.write_indent()
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

struct JunitWriter<W> {
    writer: Writer<W>,
}

impl<W: std::io::Write> JunitWriter<W> {
    fn new(output: W) -> Self {
        Self {
            writer: Writer::new_with_indent(output, b' ', 4),
        }
    }

    fn start_suite(
        &mut self,
        name: &str,
        tests: &str,
        failures: &str,
        package: &str,
    ) -> std::io::Result<()> {
        self.start(
            "testsuite",
            &[
                ("name", name),
                ("tests", tests),
                ("skipped", "0"),
                ("errors", "0"),
                ("failures", failures),
                ("package", package),
            ],
        )
    }

    fn start(&mut self, name: &str, attributes: &[(&str, &str)]) -> std::io::Result<()> {
        self.tag(name, attributes, false)
    }

    fn empty(&mut self, name: &str, attributes: &[(&str, &str)]) -> std::io::Result<()> {
        self.tag(name, attributes, true)
    }

    fn tag(&mut self, name: &str, attributes: &[(&str, &str)], empty: bool) -> std::io::Result<()> {
        let mut tag = BytesStart::new(name);
        for &(key, value) in attributes {
            let value = xml_string(value);
            tag.push_attribute((key, value.as_str()));
        }
        self.writer.write_event(if empty {
            Event::Empty(tag)
        } else {
            Event::Start(tag)
        })
    }

    fn text(&mut self, value: &str) -> std::io::Result<()> {
        let value = xml_string(value);
        self.writer.write_event(Event::Text(BytesText::new(&value)))
    }

    fn end(&mut self, name: &str) -> std::io::Result<()> {
        self.writer.write_event(Event::End(BytesEnd::new(name)))
    }
}

/// Strip terminal escapes and characters disallowed in XML 1.0, matching JUnit report output.
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
