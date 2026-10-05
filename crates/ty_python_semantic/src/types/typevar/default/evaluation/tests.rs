use std::cell::RefCell;
use std::task::Poll;

use ruff_python_ast::name::Name;

use super::*;
use crate::db::tests::setup_db;
use crate::types::TypeVarKind;
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::typevar::TypeVarIdentity;

struct Trace<'db> {
    events: RefCell<Vec<&'static str>>,
    unchecked: Option<Type<'db>>,
    self_referential: bool,
    refuse: Option<usize>,
}

impl Trace<'_> {
    fn record(&self, event: &'static str) -> Result<(), &'static str> {
        let mut events = self.events.borrow_mut();
        let index = events.len();
        events.push(event);
        if self.refuse == Some(index) {
            Err(event)
        } else {
            Ok(())
        }
    }
}

impl<'db> SynchronousTypeVarDefaultEffects<'db> for Trace<'db> {
    type Error = &'static str;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        self.record("checkpoint")
    }

    fn checked_lazy_default(
        &self,
        variable: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.record("checked_lazy_default")?;
        lazy_typevar_default_sync(variable, env, self)
    }
}

impl<'db> SynchronousLazyTypeVarDefaultEffects<'db> for Trace<'db> {
    type Error = &'static str;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        self.record("checkpoint")
    }

    fn lazy_default_unchecked(
        &self,
        _variable: TypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.record("lazy_default_unchecked")?;
        Ok(self.unchecked)
    }

    fn type_is_self_referential(
        &self,
        _variable: TypeVarInstance<'db>,
        _env: &ProgramEnvironment<'db>,
        default: Type<'db>,
    ) -> Result<bool, Self::Error> {
        self.record("type_is_self_referential")?;
        assert_eq!(Some(default), self.unchecked);
        Ok(self.self_referential)
    }
}

impl<'db> TypeVarDefaultEffects<'db> for Trace<'db> {
    type Error = &'static str;

    async fn checkpoint(&self) -> Result<(), Self::Error> {
        SynchronousTypeVarDefaultEffects::checkpoint(self)
    }

    async fn checked_lazy_default(
        &self,
        variable: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.record("checked_lazy_default")?;
        lazy_typevar_default_with(variable, env, self).await
    }
}

impl<'db> LazyTypeVarDefaultEffects<'db> for Trace<'db> {
    type Error = &'static str;

    async fn checkpoint(&self) -> Result<(), Self::Error> {
        SynchronousLazyTypeVarDefaultEffects::checkpoint(self)
    }

    async fn lazy_default_unchecked(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        SynchronousLazyTypeVarDefaultEffects::lazy_default_unchecked(self, variable)
    }

    async fn type_is_self_referential(
        &self,
        variable: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
        default: Type<'db>,
    ) -> Result<bool, Self::Error> {
        SynchronousLazyTypeVarDefaultEffects::type_is_self_referential(self, variable, env, default)
    }
}

fn variable<'db>(
    db: &'db dyn Db,
    stored: Option<TypeVarDefaultEvaluation<'db>>,
) -> TypeVarInstance<'db> {
    TypeVarInstance::new(
        db,
        TypeVarIdentity::new(db, Name::new_static("T"), None, TypeVarKind::Pep695TypeVar),
        None,
        None,
        stored,
    )
}

/// Stored dispatch and lazy validation preserve operation and refusal order in synchronous and
/// asynchronous execution.
#[test]
fn stored_defaults_preserve_checked_evaluation_and_refusal_order() {
    let db = setup_db();
    let env = db.program_environment();
    let eager = Type::int_literal(11);
    let unchecked = Type::int_literal(23);
    let direct_steps = ["checkpoint"].as_slice();
    let missing_steps = [
        "checkpoint",
        "checked_lazy_default",
        "checkpoint",
        "lazy_default_unchecked",
    ];
    let checked_steps = [
        "checkpoint",
        "checked_lazy_default",
        "checkpoint",
        "lazy_default_unchecked",
        "type_is_self_referential",
    ];
    for (stored, raw, self_referential, expected, steps) in [
        (None, Some(unchecked), false, None, direct_steps),
        (
            Some(TypeVarDefaultEvaluation::Eager(eager)),
            Some(unchecked),
            true,
            Some(eager),
            direct_steps,
        ),
        (
            Some(TypeVarDefaultEvaluation::Lazy),
            None,
            false,
            None,
            missing_steps.as_slice(),
        ),
        (
            Some(TypeVarDefaultEvaluation::Lazy),
            Some(unchecked),
            false,
            Some(unchecked),
            checked_steps.as_slice(),
        ),
        (
            Some(TypeVarDefaultEvaluation::Lazy),
            Some(unchecked),
            true,
            None,
            checked_steps.as_slice(),
        ),
    ] {
        let variable = variable(&db, stored);
        for asynchronous in [false, true] {
            for refuse in std::iter::once(None).chain((0..steps.len()).map(Some)) {
                let effects = Trace {
                    events: RefCell::default(),
                    unchecked: raw,
                    self_referential,
                    refuse,
                };
                let actual = if asynchronous {
                    try_poll_immediate(typevar_default_with(variable, &env, stored, &effects))
                } else {
                    Poll::Ready(typevar_default_sync(variable, &env, stored, &effects))
                };
                assert_eq!(
                    actual,
                    Poll::Ready(refuse.map(|index| Err(steps[index])).unwrap_or(Ok(expected)))
                );
                assert_eq!(
                    effects.events.borrow().as_slice(),
                    &steps[..refuse.map(|index| index + 1).unwrap_or(steps.len())]
                );
            }
        }
    }
}

/// Absent and eager defaults return directly without entering a supplied visitor.
#[test]
fn ordinary_stored_defaults_return_absent_or_eager_values() {
    let db = setup_db();
    let env = db.program_environment();
    let visitor = TypeVarDefaultVisitor::new(None);
    let effects = OrdinaryTypeVarDefaultEffects {
        db: &db,
        visitor: Some(&visitor),
    };
    for (stored, expected) in [
        (None, None),
        (
            Some(TypeVarDefaultEvaluation::Eager(Type::int_literal(11))),
            Some(Type::int_literal(11)),
        ),
    ] {
        assert_eq!(
            typevar_default_sync(variable(&db, stored), &env, stored, &effects),
            Ok(expected)
        );
        assert_eq!(visitor.ownership_probe_counts(), (0, 0));
    }
}
