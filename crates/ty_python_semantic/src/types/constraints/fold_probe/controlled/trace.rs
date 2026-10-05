use std::fmt::Debug;
use std::io::{self, Write};
use std::ops::ControlFlow;

use super::{RecordingControl, builder, check_publication_boundary, fold, reference};
use crate::db::tests::setup_db;
use crate::types::constraints::control::{TddError, TddWork};
use crate::types::constraints::fold_probe::inputs;
use crate::types::constraints::fold_probe::reference::{
    Value, assert_storage, fixture, fork_storage,
};
use crate::types::constraints::{
    ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder, ConstraintSetStorage,
    IteratorConstraintsExtension, SourceOrderId,
};

fn sorted_debug(values: impl Iterator<Item = impl Debug>) -> String {
    let mut values: Vec<_> = values.map(|value| format!("{value:?}")).collect();
    values.sort_unstable();
    format!("[{}]", values.join(", "))
}

// Emit full changed fields in a deterministic order. This records cache-only completions as
// well as arena appends, without repeating every unchanged arena after each public advance.
fn storage_fields(storage: &ConstraintSetStorage<'_>) -> Vec<(&'static str, String)> {
    vec![
        ("nodes", format!("{:?}", storage.nodes.raw)),
        ("supports", format!("{:?}", storage.supports.raw)),
        ("node_supports", format!("{:?}", storage.node_supports.raw)),
        (
            "constraint_supports",
            format!("{:?}", storage.constraint_supports.raw),
        ),
        ("sources", format!("{:?}", storage.source_orders.raw)),
        ("node_cache", sorted_debug(storage.node_cache.iter())),
        (
            "source_cache",
            sorted_debug(storage.source_order_cache.iter()),
        ),
        ("and_cache", sorted_debug(storage.and_cache.iter())),
        ("or_cache", sorted_debug(storage.or_cache.iter())),
        ("negate_cache", sorted_debug(storage.negate_cache.iter())),
        (
            "overlay_constraint_ids",
            sorted_debug(storage.constraint_cache.values()),
        ),
        (
            "overlay_progress",
            format!("{:?}", storage.overlay_identity_state),
        ),
        (
            "capacities_nodes_supports_node_supports_sources_node_cache_source_cache_and_or_negate",
            format!(
                "{:?}",
                [
                    storage.nodes.raw.capacity(),
                    storage.supports.raw.capacity(),
                    storage.node_supports.raw.capacity(),
                    storage.source_orders.raw.capacity(),
                    storage.node_cache.capacity(),
                    storage.source_order_cache.capacity(),
                    storage.and_cache.capacity(),
                    storage.or_cache.capacity(),
                    storage.negate_cache.capacity(),
                ]
            ),
        ),
    ]
}

#[derive(Debug, Eq, PartialEq)]
enum Outcome {
    Accepted,
    Absorbed(Value),
    Finished(Value),
}

fn trace_advances(
    output: &mut impl Write,
    builder: &ConstraintSetBuilder<'_>,
    mut advance: impl FnMut(&mut RecordingControl) -> Result<ControlFlow<Outcome>, TddError<usize>>,
) -> io::Result<Outcome> {
    let mut previous = storage_fields(&builder.storage.borrow());
    for (field, value) in &previous {
        writeln!(output, "  initial {field}={value}")?;
    }
    let mut control = RecordingControl::default();
    for index in 0..1_000 {
        control.events.clear();
        let result =
            advance(&mut control).map_err(|error| io::Error::other(format!("{error:?}")))?;
        assert!(builder.storage.try_borrow_mut().is_ok());
        check_publication_boundary(&control.events);
        if control
            .events
            .iter()
            .any(|work| matches!(work, TddWork::FoldCommit { .. }))
        {
            assert!(
                result.is_break(),
                "acceptance completes this public advance"
            );
        }
        writeln!(
            output,
            "  advance={index} work={:?} result={result:?}",
            control.events
        )?;
        let current = storage_fields(&builder.storage.borrow());
        for ((field, value), (_, old)) in current.iter().zip(&previous) {
            if value != old {
                writeln!(output, "    changed {field}={value}")?;
            }
        }
        previous = current;
        if let ControlFlow::Break(result) = result {
            return Ok(result);
        }
    }
    panic!("the bounded fold trace did not complete");
}

fn trace_case(
    output: &mut impl Write,
    name: &str,
    initial: &ConstraintSetStorage<'_>,
    kind: ConstraintFoldKind,
    prefix: &[Value],
    next: Option<Value>,
    warm: bool,
) -> io::Result<()> {
    let mut expected = reference(initial, kind);
    for value in prefix {
        assert!(expected.push(*value).is_continue());
    }
    let before = fork_storage(&expected.storage);
    let accepted = expected.accumulator.clone();
    let expected_outcome = match next {
        Some(value) => match expected.push(value) {
            ControlFlow::Continue(()) => Outcome::Accepted,
            ControlFlow::Break(value) => Outcome::Absorbed(value),
        },
        None => Outcome::Finished(expected.finish()),
    };
    let builder = builder(if warm { &expected.storage } else { &before });
    let mut actual = fold(&builder, kind, &accepted);
    let kind_name = match kind {
        ConstraintFoldKind::All => "all",
        ConstraintFoldKind::Any => "any",
    };
    writeln!(
        output,
        "TRACE {name} kind={kind_name} warm={warm} next={next:?}"
    )?;
    writeln!(
        output,
        "  accepted_before={:?} capacity={}",
        actual.accumulator,
        actual.accumulator.capacity()
    )?;
    let outcome = match next {
        Some((node, source)) => {
            let mut cursor = actual.begin_push(ConstraintSet::from_node(&builder, node, source));
            trace_advances(output, &builder, |control| {
                cursor.advance_with(control).map(|step| {
                    step.map_break(|result| match result {
                        ControlFlow::Continue(()) => Outcome::Accepted,
                        ControlFlow::Break(result) => {
                            Outcome::Absorbed((result.node, result.source_order))
                        }
                    })
                })
            })?
        }
        None => {
            let mut cursor = actual.begin_finish();
            trace_advances(output, &builder, |control| {
                cursor.advance_with(control).map(|step| {
                    step.map_break(|result| Outcome::Finished((result.node, result.source_order)))
                })
            })?
        }
    };
    assert_eq!(outcome, expected_outcome);
    assert_eq!(actual.accumulator, expected.accumulator);
    assert_storage(&builder.storage.borrow(), &expected.storage);
    writeln!(
        output,
        "  accepted_after={:?} capacity={}",
        actual.accumulator,
        actual.accumulator.capacity()
    )?;
    writeln!(output, "END {name} kind={kind_name}")
}

fn write_fold_trace(output: &mut impl Write) -> io::Result<()> {
    let db = setup_db();
    let (initial, [a, b, c, d, _, _, g, not_a, uncertain]) = fixture(&db);
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        for (name, prefix, next, warm) in [
            ("no_carry", vec![], Some(a), false),
            ("single_carry", vec![a], Some(b), false),
            ("multiple_carries", vec![a, a, a], Some(b), false),
            (
                "terminal_input",
                vec![a],
                Some((kind.absorbing(), b.1)),
                false,
            ),
            ("absorbing_carry", vec![b, c, a], Some(not_a), false),
            ("warm_carry", vec![a], Some(b), true),
            ("uncertain_carry", vec![uncertain], Some(b), false),
            ("empty_finish", vec![], None, false),
            (
                "finish_late_history",
                vec![a, a, a, a, (not_a.0, b.1), (not_a.0, b.1), d],
                None,
                false,
            ),
            ("spill_511", vec![a; 510], Some((a.0, g.1)), false),
        ] {
            trace_case(output, name, &initial, kind, &prefix, next, warm)?;
        }
    }
    let owned = ConstraintSetBuilder::new().into_owned(|builder| {
        inputs(&db, builder)
            .into_iter()
            .when_all(&db, builder, |value| value)
    });
    let mut overlay = ConstraintSetStorage {
        compacted: owned.inner.clone(),
        ..ConstraintSetStorage::default()
    };
    // Start the fold with an overlay whose constraint identities are ready but whose node
    // and source identities are still pending. Prefix construction performs no graph work.
    let mut setup_control = RecordingControl::default();
    for _ in 0..2 {
        assert!(
            overlay
                .advance_identity_caches(&mut setup_control)
                .map_err(|error| io::Error::other(format!("{error:?}")))?
                .is_continue()
        );
    }
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        trace_case(
            output,
            "partial_overlay",
            &overlay,
            kind,
            &[(owned.node, Some(SourceOrderId::from_usize(1)))],
            Some((owned.node, Some(SourceOrderId::from_usize(0)))),
            false,
        )?;
    }
    Ok(())
}

// Capture this output with the unchanged production core before storing the exact fixture.
// Full records include advance boundaries, admitted payloads and structural publications.
#[test]
#[ignore]
fn frozen_core_fold_trace_probe() -> io::Result<()> {
    write_fold_trace(&mut io::stdout().lock())
}
