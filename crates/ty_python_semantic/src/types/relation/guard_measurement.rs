use std::hint::black_box;
use std::io::{self, Write};
use std::time::Instant;

use super::TypeRelationChecker;
use super::dependencies::OrdinaryDependencies;
use super::guard::{RelationGuardStep, RelationScope};
use super::resources::RelationOwners;
use crate::db::tests::{TestDb, setup_db};
use crate::types::Type;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
use crate::types::typevar::TypeVarSet;
use crate::{Db, ProgramEnvironment};

const SAMPLES: usize = 7;
const WARM_READS: usize = 32_768;

#[derive(Clone, Copy, Debug)]
enum Case {
    ColdShallow,
    ColdSpill,
    WarmInline,
    WarmSpilled,
    ActiveDepth(usize),
}

impl Case {
    fn owners(self) -> usize {
        match self {
            Self::ColdShallow => 4_096,
            Self::ColdSpill => 1_024,
            Self::WarmInline | Self::WarmSpilled => 1,
            Self::ActiveDepth(_) => 128,
        }
    }

    fn keys(self) -> usize {
        match self {
            Self::ColdShallow | Self::WarmInline => 1,
            Self::ColdSpill | Self::WarmSpilled => 3,
            Self::ActiveDepth(depth) => depth,
        }
    }

    fn warm(self) -> bool {
        matches!(self, Self::WarmInline | Self::WarmSpilled)
    }

    fn units(self) -> usize {
        match self {
            Self::ColdShallow | Self::ColdSpill => self.owners() * self.keys(),
            Self::WarmInline | Self::WarmSpilled => WARM_READS,
            Self::ActiveDepth(_) => self.owners(),
        }
    }

    fn expected(self) -> Counts {
        let owners = self.owners();
        match self {
            Self::ColdShallow | Self::ColdSpill => {
                let calls = owners * self.keys();
                Counts {
                    requests: calls,
                    bodies: calls,
                    terminal_comparisons: calls,
                    entered: calls,
                    completed: calls,
                    never_results: calls,
                    ..Counts::default()
                }
            }
            Self::WarmInline | Self::WarmSpilled => Counts {
                requests: WARM_READS,
                hits: WARM_READS,
                never_results: WARM_READS,
                ..Counts::default()
            },
            Self::ActiveDepth(depth) => Counts {
                requests: owners * (depth + 1),
                bodies: owners,
                terminal_comparisons: owners,
                entered: owners * depth,
                completed: owners * depth,
                fallbacks: owners,
                never_results: owners * depth,
                always_results: owners,
                ..Counts::default()
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Counts {
    requests: usize,
    bodies: usize,
    terminal_comparisons: usize,
    entered: usize,
    completed: usize,
    hits: usize,
    fallbacks: usize,
    never_results: usize,
    always_results: usize,
}

struct Sample {
    elapsed_ns: u128,
    counts: Counts,
    cached_per_owner: usize,
}

fn exercise<'a, 'c, 'db>(
    db: &'db dyn Db,
    checkers: &[TypeRelationChecker<'a, 'c, 'db>],
    keys: &[Type<'db>],
    target: Type<'db>,
    case: Case,
    scratch: &mut Vec<RelationScope<'a, 'c, 'db>>,
) -> Result<Counts, &'static str> {
    let mut counts = Counts::default();
    for checker in checkers {
        let never = ConstraintSet::from_bool(checker.constraints, false);
        if let Case::ActiveDepth(depth) = case {
            for &source in &keys[..depth] {
                counts.requests += 1;
                let step = RelationGuardStep::start(
                    db,
                    checker,
                    black_box(source),
                    black_box(target),
                    &OrdinaryDependencies,
                )
                .unwrap_or_else(|never| match never {});
                match step {
                    RelationGuardStep::Evaluate(scope) => {
                        counts.entered += 1;
                        scratch.push(scope);
                    }
                    _ => {
                        while let Some(scope) = scratch.pop() {
                            drop(scope);
                        }
                        return Err("a fresh distinct literal key did not enter its guard");
                    }
                }
            }

            counts.requests += 1;
            let repeated = RelationGuardStep::start(
                db,
                checker,
                black_box(keys[depth - 1]),
                black_box(target),
                &OrdinaryDependencies,
            )
            .unwrap_or_else(|never| match never {});
            match repeated {
                RelationGuardStep::Complete(result) => {
                    counts.fallbacks += 1;
                    counts.always_results += usize::from(result.ownership_probe_same_set(
                        ConstraintSet::from_bool(checker.constraints, true),
                    ));
                }
                other => {
                    drop(other);
                    while let Some(scope) = scratch.pop() {
                        drop(scope);
                    }
                    return Err("an exact active reentry did not return the configured fallback");
                }
            }

            counts.bodies += 1;
            counts.terminal_comparisons += 1;
            let result = checker.check_type_pair(db, black_box(keys[depth - 1]), black_box(target));
            while let Some(scope) = scratch.pop() {
                let result = scope.finish(black_box(result));
                counts.completed += 1;
                counts.never_results += usize::from(result.ownership_probe_same_set(never));
            }
        } else {
            let requests = if case.warm() { WARM_READS } else { case.keys() };
            for index in 0..requests {
                let source = black_box(keys[index % case.keys()]);
                let target = black_box(target);
                counts.requests += 1;
                let previous_bodies = counts.bodies;
                let result = checker.with_recursion_guard(db, source, target, || {
                    counts.bodies += 1;
                    counts.terminal_comparisons += 1;
                    checker.check_type_pair(db, source, target)
                });
                if counts.bodies == previous_bodies {
                    counts.hits += 1;
                } else {
                    counts.entered += 1;
                    counts.completed += 1;
                }
                counts.never_results += usize::from(result.ownership_probe_same_set(never));
            }
        }
    }
    Ok(black_box(counts))
}

fn measure<'db>(
    db: &'db TestDb,
    reader: &mut TestDb,
    env: &ProgramEnvironment<'db>,
    builder: &ConstraintSetBuilder<'db>,
    keys: &[Type<'db>],
    target: Type<'db>,
    case: Case,
) -> Sample {
    let owners: Vec<_> = (0..case.owners())
        .map(|_| RelationOwners::new(env, builder))
        .collect();
    let checkers: Vec<_> = owners
        .iter()
        .map(|owner| {
            let mut checker = owner.assignability(TypeVarSet::None);
            checker.perform_expensive_checks = false;
            checker
        })
        .collect();
    let mut scratch = Vec::with_capacity(case.keys());
    let never = ConstraintSet::from_bool(builder, false);

    if case.warm() {
        for checker in &checkers {
            for &source in &keys[..case.keys()] {
                let result = checker.with_recursion_guard(db, source, target, || {
                    checker.check_type_pair(db, source, target)
                });
                assert!(result.ownership_probe_same_set(never));
            }
        }
    }

    reader.clear_salsa_events();
    let start = Instant::now();
    let outcome = exercise(db, &checkers, keys, target, case, &mut scratch);
    let elapsed_ns = start.elapsed().as_nanos();
    assert!(
        reader.take_salsa_events().is_empty(),
        "ordinary literal guard measurement must not time Salsa event logging"
    );
    let counts = outcome.expect("the finite ordinary guard fixture completes");
    assert_eq!(counts, case.expected(), "{case:?}");
    assert!(scratch.is_empty());

    for checker in &checkers {
        assert_eq!(
            checker.relation_visitor.ownership_probe_counts(),
            (0, case.keys())
        );
        let mut unexpected_bodies = 0;
        for &source in &keys[..case.keys()] {
            let result = checker.with_recursion_guard(db, source, target, || {
                unexpected_bodies += 1;
                checker.check_type_pair(db, source, target)
            });
            assert!(result.ownership_probe_same_set(never));
        }
        assert_eq!(unexpected_bodies, 0, "every completed key must be cached");
        assert_eq!(
            checker.relation_visitor.ownership_probe_counts(),
            (0, case.keys())
        );
    }
    Sample {
        elapsed_ns,
        counts,
        cached_per_owner: case.keys(),
    }
}

// Owner construction, scratch allocation, assertions, replay, logging and destruction are outside
// the windows. Cold guard allocations remain timed; scalar result/body accounting is timed equally
// in both builds. Requiring zero Salsa events excludes the TestDb observer's mutex/Vec cost.
#[test]
#[ignore = "matched ordinary guard measurement; prints exact counts and timing without thresholds"]
fn ordinary_guard_measurement() -> io::Result<()> {
    let db = setup_db();
    let mut reader = db.clone();
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let keys: Vec<_> = (0..64).map(Type::int_literal).collect();
    let target = Type::int_literal(-1);
    let reference = RelationOwners::new(&env, &builder);
    let mut checker = reference.assignability(TypeVarSet::None);
    checker.perform_expensive_checks = false;
    for &source in &keys {
        assert!(
            checker
                .check_type_pair(&db, source, target)
                .ownership_probe_same_set(ConstraintSet::from_bool(&builder, false))
        );
    }

    let mut output = io::stdout().lock();
    writeln!(
        output,
        "metadata fixture=ordinary_guard_v1 source_label={:?} manifest={:?} compiler={:?} profile={:?} arch={} os={} debug_assertions={} compiled_dev_opt={:?} compiled_test_opt={:?} samples={SAMPLES} scalar_accounting=timed salsa_events=zero setup_and_destruction=untimed depth_scratch_push_pop=timed",
        std::env::var("TY_RELATION_GUARD_SOURCE_LABEL").ok(),
        std::env::var("TY_RELATION_GUARD_MANIFEST").ok(),
        std::env::var("TY_RELATION_GUARD_COMPILER").ok(),
        std::env::var("TY_RELATION_GUARD_PROFILE").ok(),
        std::env::consts::ARCH,
        std::env::consts::OS,
        cfg!(debug_assertions),
        option_env!("CARGO_PROFILE_DEV_OPT_LEVEL"),
        option_env!("CARGO_PROFILE_TEST_OPT_LEVEL"),
    )?;

    for case in [
        Case::ColdShallow,
        Case::ColdSpill,
        Case::WarmInline,
        Case::WarmSpilled,
        Case::ActiveDepth(1),
        Case::ActiveDepth(2),
        Case::ActiveDepth(4),
        Case::ActiveDepth(8),
        Case::ActiveDepth(16),
        Case::ActiveDepth(32),
        Case::ActiveDepth(64),
    ] {
        black_box(measure(
            &db,
            &mut reader,
            &env,
            &builder,
            &keys,
            target,
            case,
        ));
        let mut times = Vec::with_capacity(SAMPLES);
        for sample in 0..SAMPLES {
            let measured = measure(&db, &mut reader, &env, &builder, &keys, target, case);
            times.push(measured.elapsed_ns);
            writeln!(
                output,
                "sample case={case:?} sample={sample} owners={} keys={} units={} unit={} elapsed_ns={} ns_per_unit={:.3} cached_per_owner={} counts={:?}",
                case.owners(),
                case.keys(),
                case.units(),
                if matches!(case, Case::ActiveDepth(_)) {
                    "chain"
                } else {
                    "request"
                },
                measured.elapsed_ns,
                measured.elapsed_ns as f64 / case.units() as f64,
                measured.cached_per_owner,
                measured.counts,
            )?;
        }
        times.sort_unstable();
        writeln!(
            output,
            "summary case={case:?} samples={SAMPLES} units={} min_ns={} median_ns={} max_ns={} median_ns_per_unit={:.3}",
            case.units(),
            times[0],
            times[SAMPLES / 2],
            times[SAMPLES - 1],
            times[SAMPLES / 2] as f64 / case.units() as f64,
        )?;
    }
    Ok(())
}
