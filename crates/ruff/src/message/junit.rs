use std::io::Write;
use std::path::Path;

use quick_junit::{NonSuccessKind, Report, TestCase, TestCaseStatus, TestSuite};

use ruff_source_file::SourceLocation;

use crate::message::{
    group_messages_by_filename, Emitter, EmitterContext, Message, MessageWithLocation,
};
use crate::registry::AsRule;

#[derive(Default)]
pub struct JunitEmitter;

impl Emitter for JunitEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        messages: &[Message],
        context: &EmitterContext,
    ) -> anyhow::Result<()> {
        let mut report = Report::new("ruff");

        if messages.is_empty() {
            let mut test_suite = TestSuite::new("ruff");
            test_suite
                .extra
                .insert("package".to_string(), "org.ruff".to_string());
            let mut case = TestCase::new("No errors found", TestCaseStatus::success());
            case.set_classname("ruff");
            test_suite.add_test_case(case);
            report.add_test_suite(test_suite);
        } else {
            for (filename, messages) in group_messages_by_filename(messages) {
                let mut test_suite = TestSuite::new(filename);
                test_suite
                    .extra
                    .insert("package".to_string(), "org.ruff".to_string());

                for message in messages {
                    let MessageWithLocation {
                        message,
                        start_location,
                    } = message;
                    let mut status = TestCaseStatus::non_success(NonSuccessKind::Failure);
                    status.set_message(message.kind.body.clone());
                    let location = if context.is_notebook(message.filename()) {
                        // We can't give a reasonable location for the structured formats,
                        // so we show one that's clearly a fallback
                        SourceLocation::default()
                    } else {
                        start_location
                    };

                    status.set_description(format!(
                        "line {row}, col {col}, {body}",
                        row = location.row,
                        col = location.column,
                        body = message.kind.body
                    ));
                    let mut case = TestCase::new(
                        format!("org.ruff.{}", message.kind.rule().noqa_code()),
                        status,
                    );
                    let file_path = Path::new(filename);
                    let file_stem = file_path.file_stem().unwrap().to_str().unwrap();
                    let classname = file_path.parent().unwrap().join(file_stem);
                    case.set_classname(classname.to_str().unwrap());
                    case.extra
                        .insert("line".to_string(), location.row.to_string());
                    case.extra
                        .insert("column".to_string(), location.column.to_string());

                    test_suite.add_test_case(case);
                }
                report.add_test_suite(test_suite);
            }
        }

        report.serialize(writer)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::tests::{capture_emitter_output, create_messages};
    use crate::message::JunitEmitter;

    #[test]
    fn output() {
        let mut emitter = JunitEmitter;
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }
}
