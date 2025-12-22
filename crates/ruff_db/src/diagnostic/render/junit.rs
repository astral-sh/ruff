use std::path::Path;
use std::{collections::BTreeMap, ops::Deref};

use quick_junit::{NonSuccessKind, Report, TestCase, TestCaseStatus, TestSuite, XmlString};

use ruff_annotate_snippets::{Level, Renderer, Snippet};
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

                let indent = " ".repeat(4 * 4);
                for diagnostic in diagnostics {
                    let DiagnosticWithLocation {
                        diagnostic,
                        start_location: location,
                    } = diagnostic;
                    let indent = " ".repeat(4 * 4);

                    let mut output = diagnostic
                        .sub_diagnostics()
                        .iter()
                        .map(|sub_diagnostic| {
                            // Hydrates the sub-diagnostic with information from the
                            // parent and formats it for rendering into the XML structure

                            let message = sub_diagnostic.concise_message().to_string();
                            let severity =
                                format!("{:?}", sub_diagnostic.inner.severity).to_lowercase();
                            let mut annotations = vec![];

                            // Add function/method/etc definition location
                            // Such as: "Function defined here: /Path/to/file.py:123:456"
                            for annotation in sub_diagnostic.annotations() {
                                let file = annotation.span.file();
                                let path = file.path(self.resolver);
                                let source = file.diagnostic_source(self.resolver);

                                // NOTE: (@cetanu)
                                // It would be possible to rename the test cases, suite,
                                // and report here, to either Ty or Ruff,
                                // but I'm not sure if this should be done.
                                //
                                // match source {
                                //     DiagnosticSource::Ty(_) => (),
                                //     DiagnosticSource::Ruff(_) => (),
                                // };

                                let sub_message = match annotation.get_message() {
                                    Some(m) => m,
                                    None => message.as_str(),
                                };

                                let mut sub_diag_loc = String::new();
                                if let Some(span) = annotation.span.range() {
                                    let loc = source.as_source_code().line_column(span.start());
                                    sub_diag_loc = format!(
                                        ":{line}:{column}",
                                        line = loc.line,
                                        column = loc.column
                                    );
                                }
                                annotations.push(format!(
                                    "{indent}{severity}: {sub_message} → {path}{sub_diag_loc}",
                                ));
                            }
                            if annotations.is_empty() {
                                annotations.push(format!("{indent}{severity}: {message}"));
                            }
                            annotations.join("\n")
                        })
                        .collect::<Vec<String>>()
                        .join("\n");

                    let code = diagnostic
                        .secondary_code()
                        .map_or_else(|| diagnostic.name(), SecondaryCode::as_str);
                    let status = TestCaseStatus::non_success(NonSuccessKind::Failure);
                    let mut case = TestCase::new(format!("org.ruff.{code}"), status);
                    let classname = Path::new(filename).with_extension("");
                    case.set_classname(classname.to_str().unwrap());
                    case.status
                        .set_message(diagnostic.concise_message().to_str());
                    case.status.set_description(format!(
                        "\n{snippet}\n{output}\n{after_indent}",
                        snippet = self.render_snippet(diagnostic, Some(indent.len())),
                        after_indent = " ".repeat(4 * 3), // <failure> tag closes one indent less
                    ));

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
                    case.status.set_description(format!(
                        "\n{indent}{diagnostic_loc}{body}{sub_diags}\n{after_indent}",
                        after_indent = " ".repeat(4 * 3), // <failure> tag closes one indent less
                        body = diagnostic.concise_message().to_str(),
                    ));
                    test_suite.add_test_case(case);
                }
                report.add_test_suite(test_suite);
            }
        }

        let adapter = FmtAdapter { fmt: f };
        report.serialize(adapter).map_err(|_| std::fmt::Error)
    }

    fn render_snippet(&self, diagnostic: &Diagnostic, indentation: Option<usize>) -> String {
        let (source_text, filename) = if let Some(span) = diagnostic.primary_span_ref() {
            let file = span.file();
            let source = file.diagnostic_source(self.resolver);
            let filename = match file {
                crate::diagnostic::UnifiedFile::Ty(file) => self.resolver.path(*file),
                crate::diagnostic::UnifiedFile::Ruff(file) => file.name(),
            };
            (
                source.as_source_code().text().to_string(),
                filename.to_string(),
            )
        } else {
            return String::new();
        };

        let mut snippet = Snippet::source(&source_text)
            .line_start(1)
            .origin(&filename);

        let mut annotations = vec![];
        if let Some(primary) = diagnostic.primary_annotation() {
            if let Some(range) = primary.get_span().range() {
                annotations.push(
                    Level::Error
                        .span(range.into())
                        // Message next to the location of the problem
                        .label(primary.get_message().unwrap_or_default()),
                );
            }
        }

        for secondary in diagnostic.secondary_annotations() {
            if let Some(range) = secondary.get_span().range() {
                annotations.push(
                    Level::Info
                        .span(range.into())
                        // Message at related location involved in the problem
                        .label(secondary.get_message().unwrap_or_default()),
                );
            }
        }

        for sub in diagnostic.sub_diagnostics() {
            if let Some(primary) = sub.primary_annotation() {
                if let Some(range) = primary.get_span().range() {
                    annotations.push(
                        Level::Help
                            .span(range.into())
                            // Help message goes here usually
                            .label(primary.get_message().unwrap_or_default()),
                    );
                }
            }
        }
        snippet = snippet.annotations(annotations);

        let message = Level::Error.title(diagnostic.body()).snippet(snippet);
        let renderer = Renderer::plain();
        let rendered = renderer.render(message).to_string();

        if let Some(indentation) = indentation {
            let indent = " ".repeat(indentation);
            rendered
                .lines()
                .map(|line| format!("{indent}{line}"))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            rendered
        }
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
        render::tests::{
            create_diagnostics, create_sub_diagnostics, create_syntax_error_diagnostics,
        },
    };

    #[test]
    fn output() {
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Junit);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn sub_diagnostics() {
        let (env, diagnostics) = create_sub_diagnostics(DiagnosticFormat::Junit);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Junit);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }
}
