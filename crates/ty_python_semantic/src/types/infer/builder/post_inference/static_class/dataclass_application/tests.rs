use std::cell::RefCell;
use std::task::Poll;

use ruff_db::files::system_path_to_file;

use super::*;
use crate::Db;
use crate::db::tests::TestDbBuilder;
use crate::place::global_symbol;
use crate::types::Type;
use crate::types::signatures::effects::try_poll_immediate;

struct Recording {
    answers: [bool; 4],
    events: RefCell<Vec<&'static str>>,
    reject_at: Option<usize>,
}

impl Recording {
    fn record(&self, event: &'static str) -> Result<(), &'static str> {
        let mut events = self.events.borrow_mut();
        let position = events.len();
        events.push(event);
        if self.reject_at == Some(position) {
            Err(event)
        } else {
            Ok(())
        }
    }
}

impl<'db> SynchronousDataclassApplicationEffects<'db> for Recording {
    type Error = &'static str;

    fn has_dataclass_params(&self, _class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.record("parameters")?;
        Ok(self.answers[0])
    }

    fn has_named_tuple_class_in_mro(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        self.record("named tuple")?;
        Ok(self.answers[1])
    }

    fn is_typed_dict(&self, _class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.record("typed dict")?;
        Ok(self.answers[2])
    }

    fn is_enum(&self, _class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.record("enum")?;
        Ok(self.answers[3])
    }

    fn report_named_tuple(&self, _class: StaticClassLiteral<'db>) -> Result<(), Self::Error> {
        self.record("report named tuple")
    }

    fn report_typed_dict(&self, _class: StaticClassLiteral<'db>) -> Result<(), Self::Error> {
        self.record("report typed dict")
    }

    fn report_enum(&self, _class: StaticClassLiteral<'db>) -> Result<(), Self::Error> {
        self.record("report enum")
    }

    fn report_protocol(&self, _class: StaticClassLiteral<'db>) -> Result<(), Self::Error> {
        self.record("report protocol")
    }
}

impl<'db> DataclassApplicationEffects<'db> for Recording {
    type Error = &'static str;

    async fn has_dataclass_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        SynchronousDataclassApplicationEffects::has_dataclass_params(self, class)
    }

    async fn has_named_tuple_class_in_mro(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        SynchronousDataclassApplicationEffects::has_named_tuple_class_in_mro(self, class)
    }

    async fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        SynchronousDataclassApplicationEffects::is_typed_dict(self, class)
    }

    async fn is_enum(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        SynchronousDataclassApplicationEffects::is_enum(self, class)
    }

    async fn report_named_tuple(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error> {
        SynchronousDataclassApplicationEffects::report_named_tuple(self, class)
    }

    async fn report_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error> {
        SynchronousDataclassApplicationEffects::report_typed_dict(self, class)
    }

    async fn report_enum(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error> {
        SynchronousDataclassApplicationEffects::report_enum(self, class)
    }

    async fn report_protocol(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error> {
        SynchronousDataclassApplicationEffects::report_protocol(self, class)
    }
}

#[test]
fn application_priority_and_refusal_match() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_file("/src/classes.py", "class Subject: pass\n")
        .build()?;
    let file = db.program_file(system_path_to_file(&db, "/src/classes.py")?);
    let class = global_symbol(&db, file, "Subject")
        .place
        .ignore_possibly_undefined()
        .and_then(Type::as_class_literal)
        .and_then(|class| class.as_static())
        .ok_or_else(|| anyhow::anyhow!("missing Subject class"))?;

    // Overlapping answers make the winning diagnostic observable independently of the
    // semantic providers; ordinary mdtests exercise classification and diagnostic contents.
    let cases: &[([bool; 4], bool, &[&str])] = &[
        ([false, true, true, true], true, &["parameters"]),
        (
            [true, true, true, true],
            true,
            &["parameters", "named tuple", "report named tuple"],
        ),
        (
            [true, false, true, true],
            true,
            &[
                "parameters",
                "named tuple",
                "typed dict",
                "report typed dict",
            ],
        ),
        (
            [true, false, false, true],
            true,
            &[
                "parameters",
                "named tuple",
                "typed dict",
                "enum",
                "report enum",
            ],
        ),
        (
            [true, false, false, false],
            true,
            &[
                "parameters",
                "named tuple",
                "typed dict",
                "enum",
                "report protocol",
            ],
        ),
        (
            [true, false, false, false],
            false,
            &["parameters", "named tuple", "typed dict", "enum"],
        ),
    ];
    for &(answers, is_protocol, expected) in cases {
        for reject_at in (0..expected.len()).map(Some).chain([None]) {
            let asynchronous = Recording {
                answers,
                events: RefCell::default(),
                reject_at,
            };
            let synchronous = Recording {
                answers,
                events: RefCell::default(),
                reject_at,
            };
            let async_result = try_poll_immediate(check_dataclass_application_with(
                class,
                is_protocol,
                &asynchronous,
            ));
            let sync_result = check_dataclass_application_sync(class, is_protocol, &synchronous);
            assert_eq!(async_result, Poll::Ready(sync_result));
            assert_eq!(
                sync_result,
                reject_at.map_or(Ok(()), |index| Err(expected[index]))
            );
            let end = reject_at.map_or(expected.len(), |index| index + 1);
            assert_eq!(*asynchronous.events.borrow(), expected[..end]);
            assert_eq!(*synchronous.events.borrow(), expected[..end]);
        }
    }
    Ok(())
}
