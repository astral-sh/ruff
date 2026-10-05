use std::cell::RefCell;
use std::hint::black_box;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use super::inputs;
use super::reference::fork_storage;
use crate::db::tests::setup_db;
use crate::types::constraints::{
    ALWAYS_FALSE, ALWAYS_TRUE, ConstraintFold, ConstraintFoldKind, ConstraintSet,
    ConstraintSetBuilder, ConstraintSetStorage, IteratorConstraintsExtension, NodeId,
    SourceOrderId,
};

type Value = (NodeId, Option<SourceOrderId>);

fn fold(builder: &ConstraintSetBuilder<'_>, kind: ConstraintFoldKind, values: &[Value]) -> Value {
    let mut fold = ConstraintFold::new(builder, kind);
    for (node, source) in values.iter().copied() {
        if let std::ops::ControlFlow::Break(result) =
            fold.push(ConstraintSet::from_node(builder, node, source))
        {
            return (result.node, result.source_order);
        }
    }
    let result = fold.finish();
    (result.node, result.source_order)
}

#[derive(Clone, Copy)]
enum Mode {
    Warm,
    ColdOperationCaches,
    ColdSidecarIdentities,
    FreshStorage,
    FreshOverlay,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::Warm => "warm",
            Self::ColdOperationCaches => "cold_operation_caches",
            Self::ColdSidecarIdentities => "cold_sidecar_identities_retained_arena",
            Self::FreshStorage => "fresh_storage",
            Self::FreshOverlay => "fresh_overlay",
        }
    }
}

fn measure(
    output: &mut impl Write,
    case: &str,
    initial: &ConstraintSetStorage<'_>,
    kind: ConstraintFoldKind,
    values: &[Value],
    mode: Mode,
) -> io::Result<()> {
    let mut builder = ConstraintSetBuilder {
        storage: RefCell::new(fork_storage(initial)),
    };
    let warm = matches!(mode, Mode::Warm);
    let iterations = if warm {
        (50_000 / (values.len() + 1)).max(100)
    } else {
        (10_000 / (values.len() + 1)).clamp(40, 2_000)
    };
    if !matches!(mode, Mode::FreshStorage | Mode::FreshOverlay) {
        black_box(fold(&builder, kind, values));
    }
    let mut starting_capacity = [0; 4];
    let elapsed = if warm {
        starting_capacity = capacities(&builder);
        let start = Instant::now();
        for _ in 0..iterations {
            black_box(fold(
                black_box(&builder),
                black_box(kind),
                black_box(values),
            ));
        }
        start.elapsed()
    } else {
        let mut elapsed = Duration::ZERO;
        for iteration in 0..iterations {
            match mode {
                Mode::ColdOperationCaches => {
                    let mut storage = builder.storage.borrow_mut();
                    storage.and_cache.clear();
                    storage.or_cache.clear();
                    storage.negate_cache.clear();
                }
                Mode::ColdSidecarIdentities => {
                    let mut storage = builder.storage.borrow_mut();
                    storage
                        .source_orders
                        .raw
                        .truncate(initial.source_orders.len());
                    storage
                        .source_order_cache
                        .clone_from(&initial.source_order_cache);
                }
                Mode::FreshStorage | Mode::FreshOverlay => {
                    builder = ConstraintSetBuilder {
                        storage: RefCell::new(fork_storage(initial)),
                    };
                }
                Mode::Warm => {}
            }
            if iteration == 0 {
                starting_capacity = capacities(&builder);
            }
            let start = Instant::now();
            black_box(fold(
                black_box(&builder),
                black_box(kind),
                black_box(values),
            ));
            elapsed += start.elapsed();
        }
        elapsed
    };
    let kind = match kind {
        ConstraintFoldKind::All => "all",
        ConstraintFoldKind::Any => "any",
    };
    let storage = builder.storage.borrow();
    writeln!(
        output,
        "fold case={case} kind={kind} mode={} inputs={} iterations={iterations} timer_pairs={} elapsed_ns={} initial_nodes={} final_nodes={} initial_sidecars={} final_sidecars={} overlay={} overlay_nodes={} overlay_sidecars={} start_sidecar_capacity={} start_sidecar_cache_capacity={} start_and_cache_capacity={} start_or_cache_capacity={}",
        mode.label(),
        values.len(),
        if warm { 1 } else { iterations },
        elapsed.as_nanos(),
        initial.nodes.len(),
        storage.nodes.len(),
        initial.source_orders.len(),
        storage.source_orders.len(),
        initial.compacted.is_some(),
        initial
            .compacted
            .as_ref()
            .map_or(0, |owned| owned.nodes.len()),
        initial
            .compacted
            .as_ref()
            .map_or(0, |owned| owned.source_orders.len()),
        starting_capacity[0],
        starting_capacity[1],
        starting_capacity[2],
        starting_capacity[3]
    )
}

fn capacities(builder: &ConstraintSetBuilder<'_>) -> [usize; 4] {
    let storage = builder.storage.borrow();
    [
        storage.source_orders.raw.capacity(),
        storage.source_order_cache.capacity(),
        storage.and_cache.capacity(),
        storage.or_cache.capacity(),
    ]
}

// Setup, cloning, cache resets, teardown, and formatting are outside measured windows. Warm
// batches use one timer pair; fresh/reset cases use one pair per fold and include that fixed
// timer overhead. Cold-operation cases retain graph identities/supports and sidecar identities.
// Cold-sidecar cases retain graph caches and arena capacity but restore the original sidecar
// cache before each timing window. Its resulting capacity is reported with the fixture.
#[test]
#[ignore]
fn fold_cost_probe() -> io::Result<()> {
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "metadata fixture=fold_v1 arch={} os={} debug_assertions={} profile_label={:?} compiled_dev_opt={:?} compiled_test_opt={:?} toolchain={:?} constraint_order={:?} fold_bytes={}",
        std::env::consts::ARCH,
        std::env::consts::OS,
        cfg!(debug_assertions),
        std::env::var("TY_FOLD_PROBE_PROFILE").ok(),
        option_env!("CARGO_PROFILE_DEV_OPT_LEVEL"),
        option_env!("CARGO_PROFILE_TEST_OPT_LEVEL"),
        option_env!("RUSTUP_TOOLCHAIN"),
        std::env::var("TY_CONSTRAINT_SET_ORDER").ok(),
        size_of::<ConstraintFold<'_, '_>>()
    )?;
    let db = setup_db();
    let builder = ConstraintSetBuilder::new();
    let values = inputs(&db, &builder).map(|value| (value.node, value.source_order));
    let initial = builder.storage.into_inner();
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        let terminal = match kind {
            ConstraintFoldKind::All => ALWAYS_FALSE,
            ConstraintFoldKind::Any => ALWAYS_TRUE,
        };
        let cases = [
            ("empty", vec![]),
            ("singleton", vec![values[0]]),
            (
                "terminal_input",
                vec![values[0], (terminal, values[1].1), values[2]],
            ),
            ("absent_history_7", vec![(values[0].0, None); 7]),
            ("equal_history_7", vec![values[0]; 7]),
            (
                "distinct_history_7",
                values.iter().map(|value| (values[0].0, value.1)).collect(),
            ),
            ("distinct_graph_7", values.to_vec()),
        ];
        for (case, inputs) in &cases {
            measure(&mut output, case, &initial, kind, inputs, Mode::Warm)?;
        }
        for length in [31, 32, 63, 64, 511, 512] {
            let inputs: Vec<_> = (0..length)
                .map(|index| (values[0].0, values[index % values.len()].1))
                .collect();
            measure(
                &mut output,
                &format!("balanced_{length}"),
                &initial,
                kind,
                &inputs,
                Mode::Warm,
            )?;
        }
        for (case, inputs) in &cases[5..] {
            for mode in [
                Mode::ColdOperationCaches,
                Mode::ColdSidecarIdentities,
                Mode::FreshStorage,
            ] {
                measure(&mut output, case, &initial, kind, inputs, mode)?;
            }
        }
    }
    let owned = ConstraintSetBuilder::new().into_owned(|builder| {
        inputs(&db, builder)
            .into_iter()
            .when_all(&db, builder, |value| value)
    });
    let overlay = ConstraintSetStorage {
        compacted: owned.inner.clone(),
        ..ConstraintSetStorage::default()
    };
    let values: Vec<_> = (0..7)
        .map(|index| (owned.node, Some(SourceOrderId::from_usize(index))))
        .collect();
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        measure(
            &mut output,
            "overlay_distinct_history_7",
            &overlay,
            kind,
            &values,
            Mode::FreshOverlay,
        )?;
    }
    Ok(())
}
