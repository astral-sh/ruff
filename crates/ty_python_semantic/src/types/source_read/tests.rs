use std::cell::{Cell, RefCell};

use ruff_python_ast::PythonVersion;
use salsa::Database as _;

use super::{SourceReadControl, UnrestrictedSourceRead, read_source};
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::types::KnownClass;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Refusal {
    Before,
    After,
}

struct RecordingControl {
    calls: Cell<usize>,
    refusal: Option<Refusal>,
}

impl RecordingControl {
    fn new(refusal: Option<Refusal>) -> Self {
        Self {
            calls: Cell::new(0),
            refusal,
        }
    }
}

impl SourceReadControl for RecordingControl {
    type Error = Refusal;

    fn check(&self) -> Result<(), Refusal> {
        let phase = if self.calls.replace(self.calls.get() + 1) == 0 {
            Refusal::Before
        } else {
            Refusal::After
        };
        if self.refusal == Some(phase) {
            Err(phase)
        } else {
            Ok(())
        }
    }
}

#[test]
fn refusal_precedes_source_read_or_result_consumption() {
    for refusal in [Refusal::Before, Refusal::After] {
        let control = RecordingControl::new(Some(refusal));
        let read = Cell::new(false);
        let consumed = Cell::new(false);
        let result = read_source(&control, || {
            read.set(true);
            None::<u8>
        })
        .inspect(|_| consumed.set(true));
        assert_eq!(result, Err(refusal));
        assert_eq!(read.get(), refusal == Refusal::After);
        assert!(!consumed.get());
        assert_eq!(control.calls.get(), if read.get() { 2 } else { 1 });
    }
}

#[test]
fn ordinary_absence_and_semantic_errors_are_successful_reads() {
    assert_eq!(
        read_source(&UnrestrictedSourceRead, || None::<u8>),
        Ok(None)
    );
    assert_eq!(
        read_source(&UnrestrictedSourceRead, || Err::<u8, _>("semantic error")),
        Ok(Err("semantic error")),
    );
}

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()
}

fn executions(db: &TestDb) -> Vec<String> {
    db.clone()
        .take_salsa_events()
        .into_iter()
        .filter_map(|event| match event.kind {
            salsa::EventKind::WillExecute { database_key } => Some(
                db.ingredient_debug_name(database_key.ingredient_index())
                    .into_owned(),
            ),
            _ => None,
        })
        .collect()
}

#[test]
fn fallible_known_class_lookup_preserves_cold_source_order() -> anyhow::Result<()> {
    let ordinary = database()?;
    let controlled = database()?;
    executions(&ordinary);
    executions(&controlled);
    let expected = KnownClass::Object
        .try_to_class_literal(&ordinary, &ordinary.program_environment())
        .ok_or_else(|| anyhow::anyhow!("object was missing"))?;
    let control = RecordingControl::new(None);
    let actual = KnownClass::Object
        .try_to_class_literal_with(&controlled, &controlled.program_environment(), &control)
        .map_err(|reason| anyhow::anyhow!("unexpected refusal: {reason:?}"))?
        .ok_or_else(|| anyhow::anyhow!("object was missing"))?;
    assert_eq!(expected.known(&ordinary), actual.known(&controlled));
    let expected_events = executions(&ordinary);
    assert!(
        expected_events
            .iter()
            .any(|name| name == "known_class_to_class_literal")
    );
    assert_eq!(executions(&controlled), expected_events);
    assert_eq!(control.calls.get(), 2);
    assert_eq!(
        KnownClass::Object.try_to_class_literal_with(
            &controlled,
            &controlled.program_environment(),
            &UnrestrictedSourceRead,
        ),
        Ok(Some(actual)),
    );
    assert!(executions(&controlled).is_empty());
    Ok(())
}

#[test]
fn refused_object_lookup_never_reaches_class_conversion() -> anyhow::Result<()> {
    for refusal in [Refusal::Before, Refusal::After] {
        let db = database()?;
        let env = db.program_environment();
        executions(&db);
        let control = RecordingControl::new(Some(refusal));
        let converted = Cell::new(false);
        let result = KnownClass::Object
            .try_to_class_literal_with(&db, &env, &control)
            .inspect(|_| {
                converted.set(true);
            });
        assert_eq!(result, Err(refusal));
        assert!(!converted.get());
        let events = executions(&db);
        assert_eq!(events.is_empty(), refusal == Refusal::Before);

        // The read boundary owns no cache. A completed source child remains reusable after
        // a caller rejects its result, and a pre-entry refusal leaves the child untouched.
        let class = KnownClass::Object
            .try_to_class_literal_with(&db, &env, &UnrestrictedSourceRead)?
            .ok_or_else(|| anyhow::anyhow!("object was missing"))?;
        assert_eq!(class.known(&db), Some(KnownClass::Object));
        assert_eq!(executions(&db).is_empty(), refusal == Refusal::After);
    }
    Ok(())
}

struct RuntimeControl<'db>(&'db TestDb);

impl SourceReadControl for RuntimeControl<'_> {
    type Error = salsa::attempt_probe::Incomplete;

    fn check(&self) -> Result<(), Self::Error> {
        salsa::attempt_probe::charge(self.0, 0)
    }
}

#[test]
fn interrupted_source_recovery_is_not_an_absent_value() -> anyhow::Result<()> {
    let db = database()?;
    let calls = RefCell::new(Vec::new());
    let control = RuntimeControl(&db);
    let attempt = |allowance| {
        salsa::attempt_probe::try_with_attempt(&db, allowance, || {
            let result = read_source(&control, || {
                calls.borrow_mut().push("source");
                // Model a source owner's internal recovery after refusing generated work.
                let _ = salsa::attempt_probe::charge(&db, 1);
                None::<u8>
            });
            if result.is_ok() {
                calls.borrow_mut().push("consume");
            }
            result
        })
    };
    assert!(matches!(
        attempt(0),
        Ok(salsa::attempt_probe::AttemptOutcome::Incomplete(
            salsa::attempt_probe::Incomplete::Allowance
        ))
    ));
    assert_eq!(*calls.borrow(), ["source"]);
    calls.borrow_mut().clear();
    assert!(matches!(
        attempt(1),
        Ok(salsa::attempt_probe::AttemptOutcome::Complete(Ok(None)))
    ));
    assert_eq!(*calls.borrow(), ["source", "consume"]);
    Ok(())
}
