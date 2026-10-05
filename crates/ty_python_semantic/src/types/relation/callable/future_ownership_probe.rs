use std::cell::{Cell, RefCell};
use std::fmt::Write;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem as _;
use ty_python_core::ProgramFile;

use super::{CallableRelationStep, TypeRelation, TypeRelationChecker};
use crate::db::tests::setup_db;
use crate::place::global_symbol;
use crate::types::call::CallArguments;
use crate::types::call::bind::{Bindings, CallableBinding};
use crate::types::callable::{CallableConversionRequest, CallableType, CallableTypes};
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::relation::{HasRelationToVisitor, IsDisjointVisitor};
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::typevar::{TypeVarSet, ownership_probe_nonce_counts};
use crate::types::{ApplyTypeMappingVisitor, Type};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Incomplete {
    UnsettledDependency,
}

type Eval<T> = Result<T, Incomplete>;

#[derive(Default)]
struct Stats {
    starts: Cell<usize>,
    prepared: Cell<usize>,
    resumed: RefCell<Vec<usize>>,
    dropped: Cell<bool>,
    expected_cache: Cell<usize>,
    nonce_counts_after_preparation: Cell<(usize, usize)>,
    nonce_counts_before_preparation: Cell<(usize, usize)>,
}

#[derive(Default)]
struct Control<'db> {
    requests: RefCell<Vec<CallableConversionRequest<'db>>>,
    answer: RefCell<Option<Eval<Option<CallableTypes<'db>>>>>,
}

struct Demand<'a, 'db> {
    control: &'a Control<'db>,
    request: CallableConversionRequest<'db>,
    registered: bool,
}

impl<'db> Future for Demand<'_, 'db> {
    type Output = Eval<Option<CallableTypes<'db>>>;

    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.registered {
            self.control.requests.borrow_mut().push(self.request);
            self.registered = true;
            return Poll::Pending;
        }
        self.control
            .answer
            .borrow_mut()
            .take()
            .map_or(Poll::Pending, Poll::Ready)
    }
}

struct VisitAudit<'a, 'c, 'db> {
    visitor: &'a HasRelationToVisitor<'db, 'c>,
    stats: &'a Stats,
}

impl Drop for VisitAudit<'_, '_, '_> {
    fn drop(&mut self) {
        assert_eq!(
            self.visitor.ownership_probe_counts(),
            (0, self.stats.expected_cache.get())
        );
        self.stats.dropped.set(true);
    }
}

async fn relation_task<'db>(
    db: &'db dyn Db,
    env: ProgramEnvironment<'db>,
    preparation: Type<'db>,
    sources: &[Type<'db>],
    target: CallableType<'db>,
    control: &Control<'db>,
    stats: &Stats,
) -> Eval<Vec<bool>> {
    stats.starts.set(stats.starts.get() + 1);
    let constraints = ConstraintSetBuilder::new();
    let arguments = CallArguments::positional([Type::int_literal(1)]);
    let Type::FunctionLiteral(function) = preparation else {
        panic!("fixture preparation is a declared function");
    };
    let signature = function.signature(db).overloads[0].clone();
    stats
        .nonce_counts_before_preparation
        .set(ownership_probe_nonce_counts());
    // Two occurrences of one generic context exercise a real fresh nonce allocation.
    let bindings = Bindings::from(CallableBinding::from_overloads(
        preparation,
        [signature.clone(), signature],
    ))
    .match_parameters(db, &env, &arguments);
    stats.prepared.set(stats.prepared.get() + 1);
    stats
        .nonce_counts_after_preparation
        .set(ownership_probe_nonce_counts());
    let relation_visitor = HasRelationToVisitor::default(&constraints);
    let disjointness_visitor = IsDisjointVisitor::default(&constraints);
    let signature_visitor = SignatureRelationVisitor::default();
    let mapping_visitor = ApplyTypeMappingVisitor::new(&env);
    let _audit = VisitAudit {
        visitor: &relation_visitor,
        stats,
    };
    let checker = TypeRelationChecker::new(
        &env,
        TypeRelation::Assignability,
        &constraints,
        TypeVarSet::None,
        &relation_visitor,
        &disjointness_visitor,
        &signature_visitor,
        &mapping_visitor,
    );
    let addresses = (
        std::ptr::from_ref(&constraints),
        std::ptr::from_ref(&checker),
        std::ptr::from_ref(&bindings),
    );
    let mut results = Vec::new();
    for (index, source) in sources.iter().copied().enumerate() {
        let CallableRelationStep::Convert(pending) =
            CallableRelationStep::start(db, &checker, source, target)
        else {
            panic!("distinct fixture functions each require an uncached visit");
        };
        assert_eq!(relation_visitor.ownership_probe_counts().0, 1);
        let callables = Demand {
            control,
            request: pending.request,
            registered: false,
        }
        .await?;
        assert_eq!(
            addresses,
            (
                std::ptr::from_ref(&constraints),
                std::ptr::from_ref(&checker),
                std::ptr::from_ref(&bindings),
            )
        );
        assert_eq!(relation_visitor.ownership_probe_counts().0, 1);
        let result = pending.resume(db, callables);
        results.push(result.is_always_satisfied(db, &env));
        stats.resumed.borrow_mut().push(index);
        stats
            .expected_cache
            .set(relation_visitor.ownership_probe_counts().1);
        assert_eq!(relation_visitor.ownership_probe_counts().0, 0);
    }
    assert!(bindings.single_element().is_some());
    Ok(results)
}

#[test]
fn relation_future_ownership_probe() -> anyhow::Result<()> {
    let mut db = setup_db();
    let mut source = String::from("def prepared[T](value: T) -> T: ...\n");
    for index in 0..16 {
        writeln!(source, "def leaf_{index}(value: int) -> int: ...")?;
    }
    db.write_file("/src/probe.py", source)?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/probe.py")?,
        env.program(&db),
    );
    let preparation = global_symbol(&db, file, "prepared").place.expect_type();
    let sources: Vec<_> = (0..16)
        .map(|index| {
            global_symbol(&db, file, &format!("leaf_{index}"))
                .place
                .expect_type()
        })
        .collect();
    let leaf = sources[0]
        .try_upcast_to_callable(&db, &env)
        .ok_or_else(|| anyhow::anyhow!("fixture leaf must be callable"))?;
    let target = *leaf
        .iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("fixture signature"))?;
    let mut cx = Context::from_waker(Waker::noop());
    for count in [1, 4, 16] {
        for reverse in [false, true] {
            let sequence: Vec<_> = if reverse {
                sources[..count].iter().copied().rev().collect()
            } else {
                sources[..count].to_vec()
            };
            for cancel_at in 0..=count {
                let control = Control::default();
                let stats = Stats::default();
                let mut task = Box::pin(relation_task(
                    &db,
                    db.program_environment(),
                    preparation,
                    &sequence,
                    target,
                    &control,
                    &stats,
                ));
                assert!(task.as_mut().poll(&mut cx).is_pending());
                for index in 0..cancel_at {
                    assert_eq!(control.requests.borrow().len(), index + 1);
                    // Polling a parked task does not register or execute its prefix again.
                    assert!(task.as_mut().poll(&mut cx).is_pending());
                    assert_eq!(control.requests.borrow().len(), index + 1);
                    *control.answer.borrow_mut() = Some(Ok(Some(leaf.clone())));
                    let polled = task.as_mut().poll(&mut cx);
                    if index + 1 == count {
                        assert_eq!(polled, Poll::Ready(Ok(vec![true; count])));
                    } else {
                        assert!(polled.is_pending());
                    }
                }
                assert_eq!(stats.starts.get(), 1);
                assert_eq!(stats.prepared.get(), 1);
                assert_eq!(*stats.resumed.borrow(), (0..cancel_at).collect::<Vec<_>>());
                assert_eq!(
                    ownership_probe_nonce_counts(),
                    stats.nonce_counts_after_preparation.get()
                );
                if cancel_at == count && !reverse {
                    let (before_starts, before_nonces) =
                        stats.nonce_counts_before_preparation.get();
                    let (after_starts, after_nonces) = stats.nonce_counts_after_preparation.get();
                    assert_eq!(
                        (after_starts - before_starts, after_nonces - before_nonces),
                        (1, 1)
                    );
                }
                drop(task);
                assert!(stats.dropped.get());
            }
        }
    }
    Ok(())
}

#[test]
fn relation_future_incomplete_is_not_false() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_dedented("/src/probe.py", "def leaf(value: int) -> int: ...")?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/probe.py")?,
        env.program(&db),
    );
    let source = global_symbol(&db, file, "leaf").place.expect_type();
    let callables = source
        .try_upcast_to_callable(&db, &env)
        .ok_or_else(|| anyhow::anyhow!("fixture leaf"))?;
    let target = *callables
        .iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("fixture signature"))?;
    let control = Control::default();
    let stats = Stats::default();
    let sources = [source];
    let mut task = Box::pin(relation_task(
        &db,
        db.program_environment(),
        source,
        &sources,
        target,
        &control,
        &stats,
    ));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(task.as_mut().poll(&mut cx).is_pending());
    *control.answer.borrow_mut() = Some(Err(Incomplete::UnsettledDependency));
    assert_eq!(
        task.as_mut().poll(&mut cx),
        Poll::Ready(Err(Incomplete::UnsettledDependency))
    );
    assert!(stats.resumed.borrow().is_empty());
    assert!(stats.dropped.get());
    assert_eq!(stats.expected_cache.get(), 0);
    Ok(())
}

#[test]
fn relation_future_sibling_cancellation_is_private() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_dedented("/src/probe.py", "def leaf(value: int) -> int: ...")?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/probe.py")?,
        env.program(&db),
    );
    let source = global_symbol(&db, file, "leaf").place.expect_type();
    let callables = source
        .try_upcast_to_callable(&db, &env)
        .ok_or_else(|| anyhow::anyhow!("fixture leaf"))?;
    let target = *callables
        .iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("fixture signature"))?;
    let sources = [source];
    for cancel_first in [false, true] {
        let first_control = Control::default();
        let second_control = Control::default();
        let first_stats = Stats::default();
        let second_stats = Stats::default();
        let mut first = Box::pin(relation_task(
            &db,
            db.program_environment(),
            source,
            &sources,
            target,
            &first_control,
            &first_stats,
        ));
        let mut second = Box::pin(relation_task(
            &db,
            db.program_environment(),
            source,
            &sources,
            target,
            &second_control,
            &second_stats,
        ));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(first.as_mut().poll(&mut cx).is_pending());
        assert!(second.as_mut().poll(&mut cx).is_pending());
        if cancel_first {
            drop(first);
            assert!(first_stats.dropped.get());
            *second_control.answer.borrow_mut() = Some(Ok(Some(callables.clone())));
            assert_eq!(second.as_mut().poll(&mut cx), Poll::Ready(Ok(vec![true])));
        } else {
            drop(second);
            assert!(second_stats.dropped.get());
            *first_control.answer.borrow_mut() = Some(Ok(Some(callables.clone())));
            assert_eq!(first.as_mut().poll(&mut cx), Poll::Ready(Ok(vec![true])));
        }
        assert!(first_stats.dropped.get());
        assert!(second_stats.dropped.get());
    }
    Ok(())
}
