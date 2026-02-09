use std::{collections::BTreeMap, ops::Deref, path::Path};

use quick_junit::{NonSuccessKind, Report, TestCase, TestCaseStatus, TestSuite, XmlString};

use ruff_source_file::LineColumn;

use crate::diagnostic::{Diagnostic, SecondaryCode, render::FileResolver};

/// A renderer for diagnostics in the [JUnit] format.
///
/// See [`junit.xsd`] for the specification in the JUnit repository and an annotated [version]
/// linked from the [`quick_junit`] docs.
///
/// [JUnit]: https://junit.org/
/// [`junit.xsd`]: https://github.com/junit-team/junit-framework/blob/2870b7d8fd5bf7c1efe489d3991d3ed3900e82bb/platform-tests/src/test/resources/jenkins-junit.xsd
/// [version]: https://llg.cubic.org/docs/junit/
/// [`quick_junit`]: https://docs.rs/quick-junit/latest/quick_junit/
pub struct JunitRenderer<'a> {
    resolver: &'a dyn FileResolver,
}

impl<'a> JunitRenderer<'a> {
    pub fn new(resolver: &'a dyn FileResolver) -> Self {
        Self { resolver }
    }

    pub(super) fn render(
        &self,
        f: &mut std::fmt::Formatter,
        diagnostics: &[Diagnostic],
    ) -> std::fmt::Result {
        let mut report = Report::new("ruff");

        if diagnostics.is_empty() {
            let mut test_suite = TestSuite::new("ruff");
            test_suite
                .extra
                .insert(XmlString::new("package"), XmlString::new("org.ruff"));
            let mut case = TestCase::new("No errors found", TestCaseStatus::success());
            case.set_classname("ruff");
            test_suite.add_test_case(case);
            report.add_test_suite(test_suite);
        } else {
            for (filename, diagnostics) in group_diagnostics_by_filename(diagnostics, self.resolver)
            {
                let mut test_suite = TestSuite::new(filename);
                test_suite
                    .extra
                    .insert(XmlString::new("package"), XmlString::new("org.ruff"));

                let classname = Path::new(filename).with_extension("");

                for diagnostic in diagnostics {
                    let DiagnosticWithLocation {
                        diagnostic,
                        start_location: location,
                    } = diagnostic;
                    let mut status = TestCaseStatus::non_success(NonSuccessKind::Failure);
                    status.set_message(diagnostic.concise_message().to_str());

                    if let Some(location) = location {
                        status.set_description(format!(
                            "line {row}, col {col}, {body}",
                            row = location.line,
                            col = location.column,
                            body = diagnostic.concise_message()
                        ));
                    } else {
                        status.set_description(diagnostic.concise_message().to_str());
                    }

                    let code = diagnostic
                        .secondary_code()
                        .map_or_else(|| diagnostic.name(), SecondaryCode::as_str);
                    let mut case = TestCase::new(format!("org.ruff.{code}"), status);
                    case.set_classname(classname.to_str().unwrap());

                    if let Some(location) = location {
                        case.extra.insert(
                            XmlString::new("line"),
                            XmlString::new(location.line.to_string()),
                        );
                        case.extra.insert(
                            XmlString::new("column"),
                            XmlString::new(location.column.to_string()),
                        );
                    }

                    test_suite.add_test_case(case);
                }
                report.add_test_suite(test_suite);
            }
        }

        let adapter = FmtAdapter { fmt: f };
        report.serialize(adapter).map_err(|_| std::fmt::Error)
    }
}

// TODO(brent) this and `group_diagnostics_by_filename` are also used by the `grouped` output
// format. I think they'd make more sense in that file, but I started here first. I'll move them to
// that module when adding the `grouped` output format.
struct DiagnosticWithLocation<'a> {
    diagnostic: &'a Diagnostic,
    start_location: Option<LineColumn>,
}

impl Deref for DiagnosticWithLocation<'_> {
    type Target = Diagnostic;

    fn deref(&self) -> &Self::Target {
        self.diagnostic
    }
}

fn group_diagnostics_by_filename<'a>(
    diagnostics: &'a [Diagnostic],
    resolver: &'a dyn FileResolver,
) -> BTreeMap<&'a str, Vec<DiagnosticWithLocation<'a>>> {
    let mut grouped_diagnostics = BTreeMap::default();
    for diagnostic in diagnostics {
        let (filename, start_location) = diagnostic
            .primary_span_ref()
            .map(|span| {
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
            })
            .unwrap_or_default();

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

struct FmtAdapter<'a> {
    fmt: &'a mut dyn std::fmt::Write,
}

impl std::io::Write for FmtAdapter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.fmt
            .write_str(std::str::from_utf8(buf).map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid UTF-8 in JUnit report",
                )
            })?)
            .map_err(std::io::Error::other)?;

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    fn write_fmt(&mut self, args: std::fmt::Arguments<'_>) -> std::io::Result<()> {
        self.fmt.write_fmt(args).map_err(std::io::Error::other)
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
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Junit);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Junit);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }
}
