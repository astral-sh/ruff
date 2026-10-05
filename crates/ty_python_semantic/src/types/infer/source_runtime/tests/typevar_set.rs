//! Exercises ordered inferable-variable construction through the production source effects.
//!
//! Rust controls are needed to compare canonical handles and interrupt independent work/byte
//! admissions. The input adapter preserves duplicate occurrences, which a GenericContext has
//! already removed, and delegates all identity, map, and interning operations to SourceEffects.

use std::array;
use std::cell::Cell;
use std::iter::Peekable;

use test_case::test_case;

use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::types::BindingContext;
use crate::types::infer::local_with_fixed_transfers_at;
use crate::types::typevar::construction::{
    TypeVarSetConstructionEffects, TypeVarSetVariables, typevar_set_from_typevars_with,
};
use crate::types::typevar::{TypeVarNonce, TypeVarSet, TypeVarSetInner};

/// Records progress without allocating or querying semantic values during construction.
#[derive(Clone, Copy, Debug, Default)]
struct Progress {
    processed: usize,
    populated_remaining: Option<usize>,
    interned: usize,
    published: usize,
}

/// Supplies duplicate-preserving input while retaining production storage and canonical effects.
struct InputEffects<'a, 'run, 'db, E, const N: usize> {
    effects: &'a E,
    endpoint: &'a TaskEndpoint<'run, 'db>,
    db: &'db dyn Db,
    progress: &'a Cell<Progress>,
}

impl<'db, E: TypeVarSetConstructionEffects<'db, Error = RunError>, const N: usize>
    TypeVarSetConstructionEffects<'db> for InputEffects<'_, '_, 'db, E, N>
{
    type Error = RunError;
    type Input = Peekable<array::IntoIter<BoundTypeVarInstance<'db>, N>>;

    async fn is_empty(&self, input: &mut Self::Input) -> RunResult<bool> {
        local_with_fixed_transfers_at(self.endpoint, 3, 0, || input.peek().is_none()).await
    }

    async fn empty(&self) -> RunResult<TypeVarSet<'db>> {
        self.effects.empty().await
    }

    async fn new_variables(&self) -> RunResult<TypeVarSetVariables<'db>> {
        self.effects.new_variables().await
    }

    async fn next_variable(
        &self,
        input: &mut Self::Input,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        local_with_fixed_transfers_at(self.endpoint, 2, 0, || input.next()).await
    }

    async fn identity(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarIdentity<'db>> {
        self.effects.identity(variable).await
    }

    async fn insert(
        &self,
        variables: &mut TypeVarSetVariables<'db>,
        identity: BoundTypeVarIdentity<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        let mut progress = self.progress.get();
        if progress.processed == 4 {
            progress.populated_remaining =
                salsa::attempt_probe::remaining_allowance_for_diagnostics(self.db);
            self.progress.set(progress);
        }
        self.effects.insert(variables, identity, variable).await?;
        progress.processed += 1;
        self.progress.set(progress);
        Ok(())
    }

    async fn shrink(&self, variables: &mut TypeVarSetVariables<'db>) -> RunResult<()> {
        self.effects.shrink(variables).await
    }

    async fn intern(&self, variables: TypeVarSetVariables<'db>) -> RunResult<TypeVarSetInner<'db>> {
        let inner = self.effects.intern(variables).await?;
        let mut progress = self.progress.get();
        progress.interned += 1;
        self.progress.set(progress);
        Ok(inner)
    }

    async fn publish(&self, inner: TypeVarSetInner<'db>) -> RunResult<TypeVarSet<'db>> {
        let set = self.effects.publish(inner).await?;
        let mut progress = self.progress.get();
        progress.published += 1;
        self.progress.set(progress);
        Ok(set)
    }
}

/// Executes the shared constructor within the existing registered source runtime.
#[derive(Debug)]
struct Construct<'db, 'progress, const N: usize> {
    variables: [BoundTypeVarInstance<'db>; N],
    progress: &'progress Cell<Progress>,
}

impl<'db, const N: usize> MemberOperation<'db> for Construct<'db, '_, N> {
    type Output = TypeVarSet<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let endpoint = access.endpoint();
        let effects =
            local_with_fixed_transfers_at(endpoint, 2, 0, || SourceEffects::new(access, program))
                .await?;
        let input = local_with_fixed_transfers_at(endpoint, N + 2, 0, || {
            self.variables.into_iter().peekable()
        })
        .await?;
        let adapter = local_with_fixed_transfers_at(endpoint, 4, 0, || InputEffects::<_, N> {
            effects: &effects,
            endpoint,
            db: access.db(),
            progress: self.progress,
        })
        .await?;
        effects
            .allocate_future(|| typevar_set_from_typevars_with(input, &adapter))
            .await?
            .await
    }
}

/// Creates canonical input handles without constructing or warming the target inferable set.
fn variables<'db, const N: usize>(
    db: &'db dyn Db,
    program: Program<'db>,
) -> [BoundTypeVarInstance<'db>; N] {
    array::from_fn(|index| {
        BoundTypeVarInstance::new(
            db,
            TypeVarInstance::new(
                db,
                TypeVarIdentity::new(
                    db,
                    Name::new(format!("T{index}")),
                    None,
                    TypeVarKind::Pep695TypeVar,
                ),
                None,
                None,
                None,
            ),
            BindingContext::Synthetic(program),
            None,
            TypeVarNonce::NONE,
        )
    })
}

/// Checks first-instance retention, encounter order, and distinct fresh occurrences canonically.
#[test]
fn duplicate_identities_keep_first_instance_and_fresh_occurrences() -> anyhow::Result<()> {
    let db = boolean_fixture(true);
    let prepared = prepare(&db);
    let program = prepared.program_file().program(&db);
    let [first, second] = variables(&db, program);
    let raw = first.typevar(&db);
    let altered = BoundTypeVarInstance::new(
        &db,
        TypeVarInstance::new(
            &db,
            raw.identity(&db),
            None,
            None,
            Some(TypeVarDefaultEvaluation::Eager(Type::bool_literal(false))),
        ),
        first.binding_context(&db),
        None,
        TypeVarNonce::NONE,
    );
    let fresh = BoundTypeVarInstance::new(
        &db,
        raw,
        first.binding_context(&db),
        None,
        TypeVarNonce::NONE.increment(),
    );
    assert_ne!(first, altered);
    assert_eq!(first.identity(&db), altered.identity(&db));
    assert_ne!(first.identity(&db), fresh.identity(&db));
    let input = [altered, second, first, fresh, second];
    let progress = Cell::new(Progress::default());
    let result = controlled_member_operation(
        &prepared,
        Construct {
            variables: input,
            progress: &progress,
        },
        &funded(),
    )
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let AnalysisOutcome::Complete(set) = result else {
        anyhow::bail!("set construction did not complete: {result:?}");
    };
    assert_eq!(set.iter(&db).collect::<Vec<_>>(), [altered, second, fresh]);
    assert_eq!(set, TypeVarSet::from_typevars(&db, input));
    assert_eq!(progress.get().processed, input.len());
    assert_eq!(progress.get().published, 1);
    assert_no_active_attempt();
    Ok(())
}

/// Checks that empty input returns the empty variant without interning an empty map.
#[test]
fn empty_input_does_not_intern_a_set() -> anyhow::Result<()> {
    let db = boolean_fixture(true);
    let prepared = prepare(&db);
    let progress = Cell::new(Progress::default());
    let result = controlled_member_operation(
        &prepared,
        Construct {
            variables: [],
            progress: &progress,
        },
        &funded(),
    )
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert_eq!(result, AnalysisOutcome::Complete(TypeVarSet::None));
    assert_eq!(progress.get().processed, 0);
    assert_eq!(progress.get().interned, 0);
    assert_eq!(progress.get().published, 0);
    assert_no_active_attempt();
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Limit {
    Work,
    Bytes,
}

/// Measures progress on a fresh database so earlier canonical sets cannot conceal admissions.
fn calibrate(policy: &AnalysisPolicy) -> anyhow::Result<Progress> {
    let db = boolean_fixture(true);
    let prepared = prepare(&db);
    let progress = Cell::new(Progress::default());
    let result = controlled_member_operation(
        &prepared,
        Construct {
            variables: variables::<12>(&db, prepared.program_file().program(&db)),
            progress: &progress,
        },
        policy,
    )
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    match result {
        AnalysisOutcome::Complete(_)
        | AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit | AnalysisIncomplete::RequestedAllocationLimit,
            ..
        } => {}
        other => anyhow::bail!("unexpected calibration result: {other:?}"),
    }
    assert_no_active_attempt();
    Ok(progress.get())
}

/// Finds an independent work or byte limit that refuses while the ordered map is populated.
fn refusal_policy(limit: Limit) -> anyhow::Result<AnalysisPolicy> {
    match limit {
        Limit::Work => {
            let remaining = calibrate(&funded())?
                .populated_remaining
                .ok_or_else(|| anyhow::anyhow!("funded construction missed the populated map"))?;
            Ok(AnalysisPolicy {
                semantic_work_limit: funded().semantic_work_limit - remaining,
                ..funded()
            })
        }
        Limit::Bytes => {
            let mut lower = 0;
            let mut upper = funded().requested_bytes_limit;
            while lower < upper {
                let middle = lower + (upper - lower) / 2;
                let progress = calibrate(&AnalysisPolicy {
                    requested_bytes_limit: middle,
                    ..funded()
                })?;
                if progress.processed >= 5 {
                    upper = middle;
                } else {
                    lower = middle + 1;
                }
            }
            Ok(AnalysisPolicy {
                requested_bytes_limit: upper
                    .checked_sub(1)
                    .ok_or_else(|| anyhow::anyhow!("set insertion required no bytes"))?,
                ..funded()
            })
        }
    }
}

/// Refuses a populated-map insertion, then checks canonical completion and reuse in the same revision.
#[test_case(Limit::Work; "independent work limit")]
#[test_case(Limit::Bytes; "independent byte limit")]
fn refused_populated_set_retries_canonically_in_same_revision(limit: Limit) -> anyhow::Result<()> {
    let policy = refusal_policy(limit)?;
    let db = boolean_fixture(true);
    let prepared = prepare(&db);
    let input = variables::<12>(&db, prepared.program_file().program(&db));
    let revision = salsa::plumbing::current_revision(&db);
    let progress = Cell::new(Progress::default());
    let refused = controlled_member_operation(
        &prepared,
        Construct {
            variables: input,
            progress: &progress,
        },
        &policy,
    )
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let expected = match limit {
        Limit::Work => AnalysisIncomplete::WorkLimit,
        Limit::Bytes => AnalysisIncomplete::RequestedAllocationLimit,
    };
    assert!(
        matches!(refused, AnalysisOutcome::Incomplete { reason, .. } if reason == expected),
        "{refused:?}"
    );
    assert_eq!(progress.get().processed, 4);
    assert_eq!(progress.get().interned, 0);
    assert_eq!(progress.get().published, 0);
    assert_no_active_attempt();

    progress.set(Progress::default());
    let result = controlled_member_operation(
        &prepared,
        Construct {
            variables: input,
            progress: &progress,
        },
        &funded(),
    )
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let AnalysisOutcome::Complete(set) = result else {
        anyhow::bail!("same-revision set retry did not complete: {result:?}");
    };
    assert_eq!(progress.get().processed, input.len());
    assert_eq!(progress.get().interned, 1);
    assert_eq!(progress.get().published, 1);
    assert_eq!(set.iter(&db).collect::<Vec<_>>(), input);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    let reused = controlled_member_operation(
        &prepared,
        Construct {
            variables: input,
            progress: &Cell::new(Progress::default()),
        },
        &funded(),
    )
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert_eq!(reused, AnalysisOutcome::Complete(set));
    assert_eq!(set, TypeVarSet::from_typevars(&db, input));
    assert_no_active_attempt();
    Ok(())
}
