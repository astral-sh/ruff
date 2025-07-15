use std::path::Path;

use quick_junit::{NonSuccessKind, Report, TestCase, TestCaseStatus, TestSuite, XmlString};

use crate::diagnostic::render::FileResolver;
use crate::diagnostic::render::grouped::{DiagnosticWithLocation, group_diagnostics_by_filename};
use crate::diagnostic::{Diagnostic, SecondaryCode};

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

                for diagnostic in diagnostics {
                    let DiagnosticWithLocation {
                        diagnostic,
                        start_location: location,
                    } = diagnostic;
                    let mut status = TestCaseStatus::non_success(NonSuccessKind::Failure);
                    status.set_message(diagnostic.body());

                    status.set_description(format!(
                        "line {row}, col {col}, {body}",
                        row = location.line,
                        col = location.column,
                        body = diagnostic.body()
                    ));
                    let code = diagnostic
                        .secondary_code()
                        .map_or_else(|| diagnostic.name(), SecondaryCode::as_str);
                    let mut case = TestCase::new(format!("org.ruff.{code}"), status);
                    let file_path = Path::new(filename);
                    let file_stem = file_path.file_stem().unwrap().to_str().unwrap();
                    let classname = file_path.parent().unwrap().join(file_stem);
                    case.set_classname(classname.to_str().unwrap());
                    case.extra.insert(
                        XmlString::new("line"),
                        XmlString::new(location.line.to_string()),
                    );
                    case.extra.insert(
                        XmlString::new("column"),
                        XmlString::new(location.column.to_string()),
                    );

                    test_suite.add_test_case(case);
                }
                report.add_test_suite(test_suite);
            }
        }

        // Safety: this should be infallible as long as the data we put in the report is valid
        // UTF-8.
        //
        // It's a bit of a shame to call `to_string`, but `Report` otherwise only exposes a
        // `serialize` method, which expects an `io::Write`, not a `fmt::Write`. `to_string`
        // currently (2025-07-15) serializes into a `Vec<u8>` and converts to a `String` from there.
        f.write_str(
            &report
                .to_string()
                .expect("Failed to serialize JUnit report"),
        )
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
