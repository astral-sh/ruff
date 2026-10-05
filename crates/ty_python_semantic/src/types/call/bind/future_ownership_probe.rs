//! Scratch ownership probe for the `ParamSpec` sub-call completion edge.
//!
//! The child `Bindings` are computed by the current synchronous binder and then delivered through
//! a controlled future. This isolates whether a suspended parent can retain its real checker.
//! The two iterations deliberately repeat this edge; they are not a replacement call algorithm.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;

use super::*;
use crate::db::tests::setup_db;
use crate::place::global_symbol;
use crate::types::typevar::ownership_probe_nonce_counts;

#[derive(Default)]
struct Control {
    released: Cell<usize>,
    incomplete_at: Cell<Option<usize>>,
    starts: Cell<usize>,
    inference_runs: Cell<usize>,
    demands: Cell<usize>,
    pending: Cell<usize>,
    cancelled: Cell<usize>,
    finished: Cell<usize>,
    completed: RefCell<Vec<usize>>,
    error_prefix_lengths: RefCell<Vec<usize>>,
}

struct Demand<'a, 'db> {
    control: &'a Control,
    index: usize,
    bindings: RefCell<Option<Bindings<'db>>>,
    returned: Cell<bool>,
}

impl<'a, 'db> Demand<'a, 'db> {
    fn new(control: &'a Control, index: usize, bindings: Bindings<'db>) -> Self {
        control.demands.set(control.demands.get() + 1);
        control.pending.set(control.pending.get() + 1);
        Self {
            control,
            index,
            bindings: RefCell::new(Some(bindings)),
            returned: Cell::new(false),
        }
    }
}

impl<'db> Future for Demand<'_, 'db> {
    type Output = Option<Bindings<'db>>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.control.released.get() <= self.index {
            return Poll::Pending;
        }
        self.returned.set(true);
        self.control.pending.set(self.control.pending.get() - 1);
        if self.control.incomplete_at.get() == Some(self.index) {
            self.bindings.borrow_mut().take();
            return Poll::Ready(None);
        }
        Poll::Ready(self.bindings.borrow_mut().take())
    }
}

impl Drop for Demand<'_, '_> {
    fn drop(&mut self) {
        if !self.returned.get() {
            self.control.pending.set(self.control.pending.get() - 1);
            self.control.cancelled.set(self.control.cancelled.get() + 1);
        }
    }
}

enum Outcome<'db> {
    Complete {
        inference: Option<TypeVarInference<'db>>,
        return_ty: Type<'db>,
    },
    Incomplete,
}

async fn check_with_owned_frame<'db>(
    mut checker: ArgumentTypeChecker<'_, 'db>,
    constraints: &ConstraintSetBuilder<'db>,
    control: &Control,
) -> anyhow::Result<Outcome<'db>> {
    control.starts.set(control.starts.get() + 1);
    control.inference_runs.set(control.inference_runs.get() + 1);
    checker.infer_specialization(constraints);
    let inference = checker.inference;
    anyhow::ensure!(
        inference.is_some(),
        "fixture should infer the real ParamSpec"
    );

    let (prefix, paramspec) = checker
        .signature
        .parameters()
        .as_paramspec_with_prefix()
        .ok_or_else(|| anyhow::anyhow!("expected a ParamSpec signature"))?;
    let prefix_len = prefix.len();
    let indices = checker.paramspec_argument_indices(prefix_len);

    // Populate the same parameter slots and diagnostics used by ordinary argument checking.
    // The retained state is consequently observable when the first child is pending.
    for relation in checker
        .argument_relations()
        .filter(|relation| relation.matched_parameter.index < prefix_len)
    {
        checker.check_argument_type(constraints, Argument::Positional, relation, None);
    }
    let parameters_before = checker.parameter_tys.to_vec();
    let errors_before = checker.errors.clone();
    anyhow::ensure!(parameters_before.iter().any(Option::is_some));
    anyhow::ensure!(
        !errors_before.is_empty(),
        "fixture should retain an earlier error"
    );
    control
        .error_prefix_lengths
        .borrow_mut()
        .push(checker.errors.len());

    for index in 0..2 {
        let (bindings, error_argument_indices) = checker
            .prepare_paramspec_sub_call(constraints, Some(&indices), paramspec, None)
            .ok_or_else(|| anyhow::anyhow!("ParamSpec sub-call was not prepared"))?;

        let completion = Demand::new(control, index, bindings).await;
        let Some(bindings) = completion else {
            // Incompleteness bypasses ordinary failure selection and diagnostic attachment.
            // This carrier models the policy boundary, not asynchronous overload evaluation.
            return Ok(Outcome::Incomplete);
        };

        checker.finish_paramspec_sub_call(
            &bindings,
            error_argument_indices.as_deref(),
            Some(&indices),
            paramspec,
            None,
        );
        anyhow::ensure!(checker.inference == inference);
        anyhow::ensure!(checker.parameter_tys == parameters_before);
        anyhow::ensure!(checker.errors.starts_with(&errors_before));
        control.completed.borrow_mut().push(index);
        control
            .error_prefix_lengths
            .borrow_mut()
            .push(checker.errors.len());
    }

    let (_, inference, return_ty) = checker.finish();
    control.finished.set(control.finished.get() + 1);
    Ok(Outcome::Complete {
        inference,
        return_ty,
    })
}

#[derive(Clone, Copy, Debug)]
enum Schedule {
    Complete,
    CancelAt(usize),
    IncompleteAt(usize),
}

#[test]
fn paramspec_future_ownership_probe() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_dedented(
        "/src/paramspec_future.py",
        r#"
from typing import Callable

def wrapper[**P](callback: Callable[P, int], head: int, *args: P.args, **kwargs: P.kwargs) -> int:
    raise NotImplementedError

def target(value: int) -> int:
    return value
"#,
    )?;
    let env = db.program_environment();
    let file = system_path_to_file(&db, "/src/paramspec_future.py")?;
    let file = ProgramFile::new(&db, file, env.program(&db));
    let wrapper = global_symbol(&db, file, "wrapper").place.expect_type();
    let target = global_symbol(&db, file, "target").place.expect_type();
    let int = KnownClass::Int.to_instance(&db, &env);
    let str = KnownClass::Str.to_instance(&db, &env);
    let target_function = target
        .as_function_literal()
        .ok_or_else(|| anyhow::anyhow!("expected a function literal"))?;
    let constraints = ConstraintSetBuilder::new();

    for schedule in [
        Schedule::Complete,
        Schedule::CancelAt(0),
        Schedule::CancelAt(1),
        Schedule::IncompleteAt(0),
        Schedule::IncompleteAt(1),
    ] {
        let arguments = CallArguments::positional([target, str, str]);
        let mut outer = wrapper
            .bindings(&db, &env)
            .match_parameters(&db, &env, &arguments);
        let mut binding = outer
            .iter_flat_mut()
            .next()
            .and_then(|callable| callable.overloads.pop())
            .ok_or_else(|| anyhow::anyhow!("expected an outer binding"))?;
        let checker = ArgumentTypeChecker::new(
            &db,
            &env,
            binding.signature_type,
            None,
            &binding.signature,
            &arguments,
            &binding.argument_matches,
            &mut binding.parameter_tys,
            TypeContext::default(),
            binding.return_ty,
            &mut binding.errors,
            false,
        );
        let control = Control::default();
        if let Schedule::IncompleteAt(index) = schedule {
            control.incomplete_at.set(Some(index));
        }
        let future = check_with_owned_frame(checker, &constraints, &control);
        let mut future = Box::pin(future);
        let mut cx = Context::from_waker(Waker::noop());

        assert!(future.as_mut().poll(&mut cx).is_pending());
        assert_eq!(control.starts.get(), 1);
        assert_eq!(control.inference_runs.get(), 1);
        assert_eq!(control.demands.get(), 1);
        assert_eq!(control.pending.get(), 1);
        assert_eq!(control.finished.get(), 0);

        let mut expected_completed = 0;
        for index in 0..2 {
            let nonce_before = ownership_probe_nonce_counts();
            for _ in 0..3 {
                assert!(future.as_mut().poll(&mut cx).is_pending());
            }
            assert_eq!(ownership_probe_nonce_counts(), nonce_before);
            assert_eq!(control.starts.get(), 1);
            assert_eq!(control.inference_runs.get(), 1);
            assert_eq!(control.demands.get(), index + 1);

            if matches!(schedule, Schedule::CancelAt(at) if at == index) {
                break;
            }

            control.released.set(index + 1);
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(result) => match result? {
                    Outcome::Incomplete => {
                        assert!(matches!(schedule, Schedule::IncompleteAt(at) if at == index));
                        assert_eq!(control.finished.get(), 0);
                        break;
                    }
                    Outcome::Complete {
                        inference,
                        return_ty,
                    } => {
                        assert!(matches!(schedule, Schedule::Complete));
                        assert_eq!(index, 1);
                        assert!(inference.is_some());
                        assert_eq!(return_ty, int);
                        expected_completed += 1;
                    }
                },
                Poll::Pending => {
                    assert_eq!(index, 0);
                    expected_completed += 1;
                }
            }
        }
        drop(future);

        assert_eq!(control.pending.get(), 0);
        assert_eq!(control.starts.get(), 1);
        assert_eq!(control.inference_runs.get(), 1);
        assert_eq!(
            control.completed.borrow().as_slice(),
            (0..expected_completed).collect::<Vec<_>>(),
        );
        assert_eq!(
            control.cancelled.get(),
            usize::from(matches!(schedule, Schedule::CancelAt(_))),
        );
        let lengths = control.error_prefix_lengths.borrow();
        assert_eq!(lengths.len(), expected_completed + 1);
        for adjacent in lengths.windows(2) {
            assert_eq!(adjacent[1], adjacent[0] + 1);
        }
        assert_eq!(binding.errors.len(), lengths[0] + expected_completed);
        let bound_callback = binding.parameter_tys[0]
            .and_then(Type::as_function_literal)
            .ok_or_else(|| anyhow::anyhow!("expected the specialized callback parameter"))?;
        assert_eq!(
            bound_callback.definition(&db),
            target_function.definition(&db)
        );
        assert_eq!(binding.parameter_tys[1], Some(str));
        for error in &binding.errors[lengths[0]..] {
            let BindingError::InvalidArgumentType {
                parameter,
                argument_index,
                expected_ty,
                provided_ty,
                parameter_source: Some(source),
                ..
            } = error
            else {
                anyhow::bail!("expected the forwarded-argument diagnostic: {error:?}");
            };
            assert_eq!(*argument_index, Some(2));
            assert_eq!(*expected_ty, int);
            assert_eq!(*provided_ty, str);
            assert_eq!(source.function, target_function);
            assert_eq!(source.overload_index, 0);
            assert_eq!(source.source_parameter_index(&db, parameter), Some(0));
        }
    }
    Ok(())
}
