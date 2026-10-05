use std::cell::RefCell;
use std::ops::ControlFlow;

use super::reference::{Value, assert_storage, fixture, fork_storage};
use crate::db::tests::setup_db;
use crate::types::constraints::control::{TddControl, TddError, TddWork};
use crate::types::constraints::fold::{FoldFinish, FoldPush, PreparedFoldFinish, PreparedFoldPush};
use crate::types::constraints::{
    ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
};

#[derive(Default)]
struct Trace {
    events: Vec<TddWork>,
    refuse: bool,
}

impl TddControl for Trace {
    type Error = ();

    fn admit(&mut self, work: TddWork) -> Result<(), Self::Error> {
        self.events.push(work);
        if self.refuse { Err(()) } else { Ok(()) }
    }
}

fn push(
    fold: &mut ConstraintFold<'_, '_>,
    value: Value,
    prepared: bool,
    control: &mut Trace,
) -> Result<ControlFlow<Value>, TddError<()>> {
    let value = ConstraintSet::from_node(fold.builder(), value.0, value.1);
    let mut cursor = if prepared {
        let mut entry = fold.prepare_push(value);
        match entry.advance_with(control)? {
            ControlFlow::Break(value) => {
                return Ok(value.map_break(|set| (set.node, set.source_order)));
            }
            ControlFlow::Continue(()) => entry.into_cursor(),
        }
    } else {
        fold.begin_push(value)
    };
    loop {
        if let ControlFlow::Break(value) = cursor.advance_with(control)? {
            return Ok(value.map_break(|set| (set.node, set.source_order)));
        }
    }
}

fn finish(
    fold: &mut ConstraintFold<'_, '_>,
    prepared: bool,
    control: &mut Trace,
) -> Result<Value, TddError<()>> {
    let mut cursor = if prepared {
        let mut entry = fold.prepare_finish();
        match entry.advance_with(control)? {
            ControlFlow::Break(value) => return Ok((value.node, value.source_order)),
            ControlFlow::Continue(()) => entry.into_cursor(),
        }
    } else {
        fold.begin_finish()
    };
    loop {
        if let ControlFlow::Break(value) = cursor.advance_with(control)? {
            return Ok((value.node, value.source_order));
        }
    }
}

#[test]
fn preparation_preserves_the_full_cursor_stream_and_canonical_state() -> Result<(), TddError<()>> {
    let db = setup_db();
    let (initial, [a, b, c, _, _, _, _, not_a, _]) = fixture(&db);
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        for values in [
            vec![],
            vec![(kind.identity(), None), a, b, c],
            vec![(kind.identity(), None), b, a, not_a, c],
            vec![a, (kind.absorbing(), b.1), c],
        ] {
            let builder = || ConstraintSetBuilder {
                storage: RefCell::new(fork_storage(&initial)),
            };
            let expected_builder = builder();
            let actual_builder = builder();
            let mut expected = ConstraintFold::new(&expected_builder, kind);
            let mut actual = ConstraintFold::new(&actual_builder, kind);
            let mut expected_trace = Trace::default();
            let mut actual_trace = Trace::default();
            let mut absorbed = false;
            for value in values {
                let expected_result = push(&mut expected, value, false, &mut expected_trace)?;
                assert_eq!(
                    push(&mut actual, value, true, &mut actual_trace)?,
                    expected_result
                );
                assert_eq!(actual.accumulator, expected.accumulator);
                assert_eq!(actual_trace.events, expected_trace.events);
                assert_storage(
                    &actual_builder.storage.borrow(),
                    &expected_builder.storage.borrow(),
                );
                if expected_result.is_break() {
                    absorbed = true;
                    break;
                }
            }
            if !absorbed {
                assert_eq!(
                    finish(&mut actual, true, &mut actual_trace)?,
                    finish(&mut expected, false, &mut expected_trace)?
                );
            }
            assert_eq!(actual_trace.events, expected_trace.events);
            assert_eq!(actual.accumulator, expected.accumulator);
            assert_storage(
                &actual_builder.storage.borrow(),
                &expected_builder.storage.borrow(),
            );
        }
    }
    Ok(())
}

#[test]
fn preparation_refusal_and_drop_leave_the_input_unaccepted() -> Result<(), TddError<()>> {
    let db = setup_db();
    let (initial, [a, b, ..]) = fixture(&db);
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        let builder = ConstraintSetBuilder {
            storage: RefCell::new(fork_storage(&initial)),
        };
        let mut fold = ConstraintFold::new(&builder, kind);
        assert!(push(&mut fold, a, true, &mut Trace::default())?.is_continue());
        let accepted = fold.accumulator.clone();
        let before = fork_storage(&builder.storage.borrow());
        let value = ConstraintSet::from_node(&builder, b.0, b.1);
        {
            let mut entry = fold.prepare_push(value);
            assert!(matches!(
                entry.advance_with(&mut Trace {
                    refuse: true,
                    ..Trace::default()
                }),
                Err(TddError::Refused(()))
            ));
        }
        assert_eq!(fold.accumulator, accepted);
        {
            let mut entry = fold.prepare_push(value);
            assert!(entry.advance_with(&mut Trace::default())?.is_continue());
        }
        assert_eq!(fold.accumulator, accepted);
        assert_storage(&builder.storage.borrow(), &before);
        assert!(push(&mut fold, b, true, &mut Trace::default())?.is_continue());
    }
    Ok(())
}

#[test]
fn prepared_representations_do_not_embed_graph_cursors() {
    assert!(size_of::<PreparedFoldPush<'_, '_, '_>>() < size_of::<FoldPush<'_, '_, '_>>());
    assert!(size_of::<PreparedFoldFinish<'_, '_, '_>>() < size_of::<FoldFinish<'_, '_, '_>>());
}
