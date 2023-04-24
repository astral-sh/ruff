use crate::message::{group_messages_by_filename, Emitter, EmitterContext, Message};
use crate::registry::AsRule;
use quick_junit::{NonSuccessKind, Report, TestCase, TestCaseStatus, TestSuite};
use std::io::Write;
use std::path::Path;

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

        for (filename, messages) in group_messages_by_filename(messages) {
            let mut test_suite = TestSuite::new(filename);
            test_suite
                .extra
                .insert("package".to_string(), "org.ruff".to_string());

            for message in messages {
                let mut status = TestCaseStatus::non_success(NonSuccessKind::Failure);
                status.set_message(message.kind.body.clone());
                let (row, col) = if context.is_jupyter_notebook(message.filename()) {
                    // We can't give a reasonable location for the structured formats,
                    // so we show one that's clearly a fallback
                    (1, 0)
                } else {
                    (message.location.row(), message.location.column())
                };

                status.set_description(format!("line {row}, col {col}, {}", message.kind.body));
                let mut case = TestCase::new(
                    format!("org.ruff.{}", message.kind.rule().noqa_code()),
                    status,
                );
                let file_path = Path::new(filename);
                let file_stem = file_path.file_stem().unwrap().to_str().unwrap();
                let classname = file_path.parent().unwrap().join(file_stem);
                case.set_classname(classname.to_str().unwrap());
                case.extra
                    .insert("line".to_string(), message.location.row().to_string());
                case.extra
                    .insert("column".to_string(), message.location.column().to_string());

                test_suite.add_test_case(case);
            }
            report.add_test_suite(test_suite);
        }

        report.serialize(writer)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::message::tests::{capture_emitter_output, create_messages};
    use crate::message::JunitEmitter;
    use insta::assert_snapshot;

    #[test]
    fn output() {
        let mut emitter = JunitEmitter::default();
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }
}
