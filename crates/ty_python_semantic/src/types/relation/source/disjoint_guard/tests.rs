use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

use ruff_db::files::system_path_to_file;
use ruff_python_ast::name::Name;
use salsa::Database;
use salsa::attempt_probe::Incomplete;
use salsa::execution_probe::{ExecutionAdmission, ExecutionWork, RegistryBuilder, TaskEndpoint};
use ty_python_core::{ProgramFile, Truthiness};

use super::*;
use crate::ProgramEnvironment;
use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
use crate::place::global_symbol;
use crate::types::class_base::ClassBase;
use crate::types::constraints::source::SourceStructuralResult;
use crate::types::constraints::{ConstraintSetBuilder, OwnedConstraintSet};
use crate::types::constructor::expansion_probe;
use crate::types::mro::iteration::MroCursor;
use crate::types::relation::source::resources::{
    ClassRelation, EnvironmentResourceAccess, RelationResourceAccess,
};
use crate::types::relation::source::retained::RetainedRelationSource;
use crate::types::relation::source::{FreshRelation, RelationSourceOperation};
use crate::types::relation::{IsDisjointVisitor, RelationOwners};
use crate::types::set_theoretic::builder::intersection_insertion::Elements;
use crate::types::tuple::buffer::TupleBufferStorageEffects;
use crate::types::tuple::{TupleSpec, VariableSegment};
use crate::types::typevar::TypeVarSet;
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, ClassType, FunctionType, IntersectionType, KnownClass,
    MaterializationKind, NominalInstanceType, SubclassOfInner, TypeVarVariance, UnionType,
};

#[derive(Clone)]
pub(in crate::types::relation::source) struct Effects<'run, 'db: 'run> {
    pub(in crate::types::relation::source) endpoint: TaskEndpoint<'run, 'db>,
}

#[derive(Clone, Copy)]
pub(in crate::types::relation::source) struct NoResources;

thread_local! {
    static REFUSED_OPERATION: Cell<Option<RelationSourceOperation>> = const { Cell::new(None) };
}

pub(in crate::types::relation::source) fn take_refused_operation() -> Option<RelationSourceOperation>
{
    REFUSED_OPERATION.take()
}

impl<'run, 'db: 'run> EnvironmentResourceAccess<'run, 'db> for NoResources {
    async fn retain_environment(
        self,
        _endpoint: &TaskEndpoint<'run, 'db>,
        _env: &ProgramEnvironment<'db>,
    ) -> RunResult<&'run ProgramEnvironment<'db>> {
        Err(RunError::Contract(
            "guard test requested a retained environment",
        ))
    }
}

impl<'run, 'db: 'run> RelationResourceAccess<'run, 'db> for NoResources {
    type Builder = &'run ConstraintSetBuilder<'db>;

    async fn owned_assignability<E: RelationSourceEffects<'run, 'db>>(
        self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _source: Type<'db>,
        _target: Type<'db>,
        _effects: &E,
    ) -> RunResult<OwnedConstraintSet<'db>> {
        Err(RunError::Contract("guard test requested owned assignability"))
    }

    async fn intersect_owned_terminals<E: RelationSourceEffects<'run, 'db>>(
        self,
        _db: &'db dyn Db,
        _first: &OwnedConstraintSet<'db>,
        _second: &OwnedConstraintSet<'db>,
        _effects: &E,
    ) -> RunResult<SourceStructuralResult<OwnedConstraintSet<'db>>> {
        Err(RunError::Contract("guard test requested owned intersection"))
    }

    async fn invocation_builder(
        self,
        _endpoint: &TaskEndpoint<'run, 'db>,
    ) -> RunResult<Self::Builder> {
        Err(RunError::Contract(
            "guard test requested an invocation builder",
        ))
    }

    async fn assignability<E: RelationSourceEffects<'run, 'db>>(
        self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _constraints: Self::Builder,
        _source: Type<'db>,
        _target: Type<'db>,
        _inferable: TypeVarSet<'db>,
        _always: bool,
        _effects: &E,
    ) -> RunResult<bool> {
        Err(RunError::Contract("guard test requested assignability"))
    }

    async fn condition<E: RelationSourceEffects<'run, 'db>>(
        self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _source: Type<'db>,
        _target: Type<'db>,
        _relation: FreshRelation,
        _effects: &E,
    ) -> RunResult<bool> {
        Err(RunError::Contract("guard test requested fresh owners"))
    }

    async fn class_condition<E: RelationSourceEffects<'run, 'db>>(
        self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _source: ClassType<'db>,
        _target: ClassType<'db>,
        _relation: ClassRelation,
        _effects: &E,
    ) -> RunResult<bool> {
        Err(RunError::Contract("guard test requested a class condition"))
    }

    async fn equivalence<E: RelationSourceEffects<'run, 'db>>(
        self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _source: Type<'db>,
        _target: Type<'db>,
        _effects: &E,
    ) -> RunResult<bool> {
        Err(RunError::Contract("guard test requested equivalence"))
    }
}

impl<'run, 'db: 'run> RetainedRelationSource<'run, 'db> for Effects<'run, 'db> {
    type Effects<'call>
        = Self
    where
        Self: 'call;

    fn effects(&self) -> Self {
        self.clone()
    }
}

macro_rules! unavailable_effects {
    ($(fn $name:ident($($parameter:ident: $argument:ty),*) -> $result:ty;)*) => {
        $(
            async fn $name(&self, $($parameter: $argument),*) -> RunResult<$result> {
                $(let _ = $parameter;)*
                Err(RunError::Contract("guard test requested a semantic dependency"))
            }
        )*
    };
}

impl<'run, 'db: 'run> TupleBufferStorageEffects<'db> for Effects<'run, 'db> {
    unavailable_effects! {
        fn new_elements(capacity: usize) -> Vec<Type<'db>>;
        fn push_element(elements: &mut Vec<Type<'db>>, ty: Type<'db>) -> ();
        fn finish_elements(elements: &mut Vec<Type<'db>>, variable: Option<(usize, VariableSegment<'db>)>) -> TupleSpec<'db>;
    }
}

impl<'run, 'db: 'run> RelationSourceEffects<'run, 'db> for Effects<'run, 'db> {
    type Resources = NoResources;
    type Retained = Self;

    fn resources(&self) -> Self::Resources {
        NoResources
    }

    fn retained(&self) -> Self::Retained {
        self.clone()
    }

    fn endpoint(&self) -> &TaskEndpoint<'run, 'db> {
        &self.endpoint
    }

    async fn unavailable<T>(&self, operation: RelationSourceOperation) -> RunResult<T> {
        REFUSED_OPERATION.set(Some(operation));
        Err(RunError::Contract(
            "guard test reached an unavailable operation",
        ))
    }

    unavailable_effects! {
        fn type_truthiness(env: &ProgramEnvironment<'db>, ty: Type<'db>) -> Truthiness;
        fn nominal_class(env: &ProgramEnvironment<'db>, instance: NominalInstanceType<'db>) -> ClassType<'db>;
        fn nominal_is_definition_generic(instance: NominalInstanceType<'db>) -> bool;
        fn nominal_known_class(instance: NominalInstanceType<'db>) -> Option<KnownClass>;
        fn function_runtime_class(function: FunctionType<'db>) -> KnownClass;
        fn known_class_instance(env: &ProgramEnvironment<'db>, class: KnownClass) -> Type<'db>;
        fn cached_materialization(env: &ProgramEnvironment<'db>, ty: Type<'db>, kind: MaterializationKind) -> Type<'db>;
        fn union_elements(union: UnionType<'db>) -> &'db [Type<'db>];
        fn intersection_positive_contains(intersection: IntersectionType<'db>, ty: Type<'db>) -> bool;
        fn intersection_negative_contains(intersection: IntersectionType<'db>, ty: Type<'db>) -> bool;
        fn intersection_positive_elements(intersection: IntersectionType<'db>) -> Elements<'db>;
        fn intersection_negative_elements(intersection: IntersectionType<'db>) -> Elements<'db>;
        fn intersection_next_element(elements: &mut Elements<'db>) -> Option<Type<'db>>;
        fn intersection_alternatives(env: &ProgramEnvironment<'db>, intersection: IntersectionType<'db>) -> Option<Type<'db>>;
        fn intersection_expand(env: &ProgramEnvironment<'db>, intersection: IntersectionType<'db>) -> Type<'db>;
        fn class_default_specialization(class: ClassLiteral<'db>) -> ClassType<'db>;
        fn subclass_inner_class(inner: SubclassOfInner<'db>) -> Option<ClassType<'db>>;
        fn class_mro_start(class: ClassType<'db>) -> MroCursor<'db>;
        fn class_mro_next(cursor: &mut MroCursor<'db>) -> Option<ClassBase<'db>>;
        fn class_is_object(class: ClassType<'db>) -> bool;
        fn class_is_final(class: ClassType<'db>) -> bool;
    }
}

#[derive(Clone, Copy)]
pub(in crate::types::relation::source) enum Failure {
    Refuse,
    Cancel,
}

pub(in crate::types::relation::source) struct Admission<'db> {
    db: &'db TestDb,
    failure: Failure,
    pub(in crate::types::relation::source) armed: Cell<bool>,
    pub(in crate::types::relation::source) fired: Cell<bool>,
}

impl ExecutionAdmission for Admission<'_> {
    fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
        if self.armed.replace(false) {
            self.fired.set(true);
            match self.failure {
                Failure::Refuse => return Err(RunError::Refused(Incomplete::Allowance)),
                Failure::Cancel => self.db.cancellation_token().cancel(),
            }
        }
        Ok(())
    }
}

pub(in crate::types::relation::source) fn make_admission(
    db: &TestDb,
    failure: Failure,
) -> Admission<'_> {
    Admission {
        db,
        failure,
        armed: Cell::new(false),
        fired: Cell::new(false),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CandidateComponent {
    Source,
    Target,
}

// These controls exercise the production disjoint guard's candidate adapter and cache ownership.
// Mdtests cannot distinguish a field-free candidate rejection from an unavailable field request.
#[test_case::test_case(CandidateComponent::Source; "source differs in variant")]
#[test_case::test_case(CandidateComponent::Target; "target differs in variant")]
fn different_variants_do_not_request_disjoint_identity(
    component: CandidateComponent,
) -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_file("/src/disjoint.py", "def function(): ...\n")
        .build()?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/disjoint.py")?,
        env.program(&db),
    );
    let function = global_symbol(&db, file, "function").place.expect_type();
    assert!(matches!(function, Type::FunctionLiteral(_)));
    let one = Type::int_literal(1);
    let (outer, inner) = match component {
        CandidateComponent::Source => ((Type::object(), one), (function, one)),
        CandidateComponent::Target => ((one, Type::object()), (one, function)),
    };
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let checker = owners.disjointness(TypeVarSet::None);
    let admission = make_admission(&db, Failure::Refuse);
    let body_ran = Cell::new(false);
    let outcome = expansion_probe::run(&db, usize::MAX, || {
        let (db, checker, builder, body_ran) = (&db, &checker, &builder, &body_ran);
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(|endpoint| async move {
                let effects = Effects { endpoint };
                let yes = ConstraintSet::from_bool(builder, true);
                take_refused_operation();
                with_guard(db, checker, outer.0, outer.1, &effects, || async {
                    with_guard(db, checker, inner.0, inner.1, &effects, || async {
                        body_ran.set(true);
                        assert_eq!(
                            checker.disjointness_visitor.ownership_probe_counts(),
                            (2, 0)
                        );
                        Ok(yes)
                    })
                    .await
                })
                .await?;
                assert_eq!(take_refused_operation(), None);
                Ok(())
            })
    })
    .0;
    assert_eq!(outcome, Ok(Ok(())));
    assert!(body_ran.get());
    assert_eq!(
        checker.disjointness_visitor.ownership_probe_counts(),
        (0, 2)
    );
    Ok(())
}

#[test]
fn repeated_active_pair_uses_false_without_caching_the_cycle() {
    let db = setup_db();
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let checker = owners.disjointness(TypeVarSet::None);
    let admission = make_admission(&db, Failure::Refuse);
    let expected = ConstraintSet::from_bool(&builder, true);
    let false_result = ConstraintSet::from_bool(&builder, false);
    let result = expansion_probe::run(&db, usize::MAX, || {
        let checker = &checker;
        let db = &db;
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(|endpoint| async move {
                let effects = Effects { endpoint };
                with_guard(
                    db,
                    checker,
                    Type::AlwaysTruthy,
                    Type::AlwaysFalsy,
                    &effects,
                    || async {
                        assert_eq!(
                            checker.disjointness_visitor.ownership_probe_counts(),
                            (1, 0)
                        );
                        let cycle = with_guard(
                            db,
                            checker,
                            Type::AlwaysTruthy,
                            Type::AlwaysFalsy,
                            &effects,
                            || async { Err(RunError::Contract("exact cycle evaluated its body")) },
                        )
                        .await?;
                        assert!(cycle.has_same_identity(false_result));
                        assert_eq!(
                            checker.disjointness_visitor.ownership_probe_counts(),
                            (1, 0)
                        );
                        Ok(expected)
                    },
                )
                .await
            })
    })
    .0;
    assert!(
        result
            .expect("completed attempt")
            .expect("completed guard")
            .has_same_identity(expected)
    );
    assert_eq!(
        checker.disjointness_visitor.ownership_probe_counts(),
        (0, 1)
    );
}

#[test]
fn completed_cache_preserves_builder_and_source_order_identity() {
    let db = setup_db();
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let checker = owners.disjointness(TypeVarSet::None);
    let admission = make_admission(&db, Failure::Refuse);
    let [left, right] = ["A", "B"].map(|name| {
        let variable = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static(name),
            TypeVarVariance::Invariant,
        );
        ConstraintSet::constrain_typevar_equivalence_bound(
            &db,
            &env,
            &builder,
            variable,
            Type::bool_literal(true),
        )
    });
    let expected = left.and(&db, &builder, || right);
    let reversed = right.and(&db, &builder, || left);
    assert!(!expected.is_source_free_terminal());
    assert!(!expected.has_same_identity(reversed));
    let another_builder = ConstraintSetBuilder::new();
    assert!(
        !ConstraintSet::from_bool(&builder, true)
            .has_same_identity(ConstraintSet::from_bool(&another_builder, true))
    );
    let computed = Cell::new(0);
    let result = expansion_probe::run(&db, usize::MAX, || {
        let checker = &checker;
        let db = &db;
        let computed = &computed;
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(|endpoint| async move {
                let effects = Effects { endpoint };
                for key in [
                    Type::AlwaysTruthy,
                    Type::AlwaysFalsy,
                    Type::object(),
                    Type::Never,
                ] {
                    let actual = with_guard(
                        db,
                        checker,
                        key,
                        Type::literal_string(),
                        &effects,
                        || async {
                            computed.set(computed.get() + 1);
                            Ok(expected)
                        },
                    )
                    .await?;
                    assert!(actual.has_same_identity(expected));
                    let cached = with_guard(
                        db,
                        checker,
                        key,
                        Type::literal_string(),
                        &effects,
                        || async {
                            Err(RunError::Contract(
                                "completed guard recomputed its cached result",
                            ))
                        },
                    )
                    .await?;
                    assert!(cached.has_same_identity(expected));
                    assert!(!cached.has_same_identity(reversed));
                }
                Ok(())
            })
    })
    .0;
    result
        .expect("completed attempt")
        .expect("completed cached guards");
    assert_eq!(computed.get(), 4);
    assert_eq!(
        checker.disjointness_visitor.ownership_probe_counts(),
        (0, 4)
    );
    assert!(
        checker
            .disjointness_visitor
            .ownership_probe_storage()
            .cache_capacity
            .is_some()
    );
}

struct PendingChild<'a, 'db, 'c> {
    visitor: &'a IsDisjointVisitor<'db, 'c>,
    dropped: &'a Cell<usize>,
    active_at_drop: &'a Cell<Option<(usize, usize)>>,
}

impl Drop for PendingChild<'_, '_, '_> {
    fn drop(&mut self) {
        self.dropped.set(self.dropped.get() + 1);
        self.active_at_drop
            .set(Some(self.visitor.ownership_probe_counts()));
    }
}

fn interrupted_guard(failure: Failure) {
    let db = setup_db();
    let retry_db = db.clone();
    let revision = salsa::plumbing::current_revision(&db);
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let checker = owners.disjointness(TypeVarSet::None);
    let admission = make_admission(&db, failure);
    let dropped = Cell::new(0);
    let child_started = Cell::new(false);
    let active_at_drop = Cell::new(None);
    let after_failure = Cell::new(false);
    let expected = ConstraintSet::from_bool(&builder, true);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        expansion_probe::run(&db, usize::MAX, || {
            let checker = &checker;
            let db = &db;
            let admission = &admission;
            let dropped = &dropped;
            let child_started = &child_started;
            let active_at_drop = &active_at_drop;
            let after_failure = &after_failure;
            RegistryBuilder::new(db, admission)?
                .seal()?
                .run(|endpoint| async move {
                    let effects = Effects { endpoint };
                    with_guard(
                        db,
                        checker,
                        Type::AlwaysTruthy,
                        Type::AlwaysFalsy,
                        &effects,
                        || async {
                            assert_eq!(
                                checker.disjointness_visitor.ownership_probe_counts(),
                                (1, 0)
                            );
                            effects
                                .endpoint
                                .local_call(|| {
                                    let child = PendingChild {
                                        visitor: checker.disjointness_visitor,
                                        dropped,
                                        active_at_drop,
                                    };
                                    let _reply = effects.endpoint.demand(move || {
                                        child_started.set(true);
                                        async move {
                                            let _child = child;
                                            Err::<(), _>(RunError::Contract(
                                                "interrupted guard ran its queued child",
                                            ))
                                        }
                                    })?;
                                    admission.armed.set(true);
                                    effects.endpoint.admit_work(1)?;
                                    effects.endpoint.check_completion()?;
                                    Ok(())
                                })
                                .await;
                            after_failure.set(true);
                            Ok(expected)
                        },
                    )
                    .await
                })
        })
        .0
    }));
    assert!(admission.fired.get());
    match failure {
        Failure::Refuse => assert!(matches!(
            outcome.expect("refusal does not unwind"),
            Err(expansion_probe::Incomplete::Allowance)
        )),
        Failure::Cancel => {
            let payload = outcome.expect_err("native cancellation unwinds");
            assert!(matches!(
                payload.downcast_ref::<salsa::Cancelled>(),
                Some(salsa::Cancelled::Local)
            ));
        }
    }
    assert!(!after_failure.get());
    assert!(!child_started.get());
    assert_eq!(dropped.get(), 1);
    assert_eq!(active_at_drop.get(), Some((1, 0)));
    assert_eq!(
        checker.disjointness_visitor.ownership_probe_counts(),
        (0, 0)
    );

    // A cloned database handle has a fresh local cancellation token and the same revision.
    let retry_db = match failure {
        Failure::Refuse => &db,
        Failure::Cancel => &retry_db,
    };
    assert_eq!(salsa::plumbing::current_revision(retry_db), revision);
    let retry_admission = make_admission(retry_db, Failure::Refuse);
    let retried = Cell::new(false);
    let result = expansion_probe::run(retry_db, usize::MAX, || {
        let checker = &checker;
        let retried = &retried;
        RegistryBuilder::new(retry_db, &retry_admission)?
            .seal()?
            .run(|endpoint| async move {
                let effects = Effects { endpoint };
                with_guard(
                    retry_db,
                    checker,
                    Type::AlwaysTruthy,
                    Type::AlwaysFalsy,
                    &effects,
                    || async {
                        retried.set(true);
                        assert_eq!(
                            checker.disjointness_visitor.ownership_probe_counts(),
                            (1, 0)
                        );
                        Ok(expected)
                    },
                )
                .await
            })
    })
    .0;
    assert!(
        result
            .expect("completed retry attempt")
            .expect("completed retry guard")
            .has_same_identity(expected)
    );
    assert!(retried.get());
    assert_eq!(
        checker.disjointness_visitor.ownership_probe_counts(),
        (0, 1)
    );
    assert_eq!(salsa::plumbing::current_revision(retry_db), revision);
}

#[test]
fn refusal_drains_children_before_retiring_the_guard_and_allows_retry() {
    interrupted_guard(Failure::Refuse);
}

#[test]
fn native_cancellation_drains_children_before_retiring_the_guard_and_allows_retry() {
    interrupted_guard(Failure::Cancel);
}
