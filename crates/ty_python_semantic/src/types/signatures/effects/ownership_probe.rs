//! An effect retains the actual signature-local checker view after comparison unwinds.
//!
//! Structural operations retain ordinary inline execution while this probe captures relation
//! dependencies; they do not add another queue or change the captured checker's resources.

use std::cell::RefCell;
use std::future::{Future, ready};
use std::ops::ControlFlow;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use ty_python_core::ProgramFile;

use super::{
    ConstraintBound, LegacyInlineEffects, SignatureEffects, SignatureVisit, sealed,
    try_poll_immediate,
};
use crate::Db;
use crate::db::tests::TestDbBuilder;
use crate::place::global_symbol;
use crate::types::constraints::{
    ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
};
use crate::types::generics::GenericContext;
use crate::types::relation::{HasRelationToVisitor, IsDisjointVisitor, TypeRelationChecker};
use crate::types::relation_error::ErrorContextTree;
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::typevar::{TypeVarNonce, TypeVarSet};
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, Parameter, Parameters, Signature, Type,
    UnionBuilder,
};

#[derive(Debug, Eq, PartialEq)]
struct Captured;

struct RetainedRelation<'state, 'db, 'c> {
    checker: TypeRelationChecker<'state, 'c, 'db>,
    source: Type<'db>,
    target: Type<'db>,
    live_result: ConstraintSet<'db, 'c>,
}

#[derive(Default)]
struct CaptureRelation<'state, 'db, 'c> {
    retained: RefCell<Option<RetainedRelation<'state, 'db, 'c>>>,
}

impl sealed::Sealed for CaptureRelation<'_, '_, '_> {}

macro_rules! forward_inline {
    ($(fn $method:ident($($argument:ident: $ty:ty),*) -> $output:ty;)*) => {
        $(async fn $method(
            &self,
            db: &'db dyn Db,
            checker: &TypeRelationChecker<'state, 'c, 'db>,
            $($argument: $ty,)*
        ) -> Result<$output, Captured> {
            LegacyInlineEffects.$method(db, checker, $($argument),*)
                .await.map_err(|never| match never {})
        })*
    };
}

impl<'state, 'db, 'c> SignatureEffects<'state, 'db, 'c> for CaptureRelation<'state, 'db, 'c> {
    type Error = Captured;

    async fn combine_constraints(
        &self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Captured> {
        LegacyInlineEffects
            .combine_constraints(db, builder, kind, left, right)
            .await
            .map_err(|never| match never {})
    }

    async fn push_constraints(
        &self,
        db: &'db dyn Db,
        fold: &mut ConstraintFold<'db, 'c>,
        next: ConstraintSet<'db, 'c>,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Captured> {
        LegacyInlineEffects
            .push_constraints(db, fold, next)
            .await
            .map_err(|never| match never {})
    }

    async fn finish_constraints(
        &self,
        db: &'db dyn Db,
        fold: &mut ConstraintFold<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Captured> {
        LegacyInlineEffects
            .finish_constraints(db, fold)
            .await
            .map_err(|never| match never {})
    }

    async fn begin_signature_visit<'visit>(
        &self,
        checker: &'visit TypeRelationChecker<'state, 'c, 'db>,
        source: &Signature<'db>,
        target: &Signature<'db>,
    ) -> Result<SignatureVisit<'visit, 'db>, Captured> {
        LegacyInlineEffects
            .begin_signature_visit(checker, source, target)
            .await
            .map_err(|never| match never {})
    }

    fn relate(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Captured>> {
        let live_result = checker.check_type_pair(db, source, target);
        let previous = self.retained.replace(Some(RetainedRelation {
            checker: checker.clone(),
            source,
            target,
            live_result,
        }));
        assert!(previous.is_none());
        ready(Err(Captured))
    }

    forward_inline! {
        fn disjoint(source: Type<'db>, target: Type<'db>) -> ConstraintSet<'db, 'c>;
        fn is_never(constraints: ConstraintSet<'db, 'c>) -> bool;
        fn is_always(constraints: ConstraintSet<'db, 'c>) -> bool;
        fn constraint_bound(kind: ConstraintBound, typevar: BoundTypeVarInstance<'db>, bound: Type<'db>) -> ConstraintSet<'db, 'c>;
        fn receiver_constraints(signature: &Signature<'db>) -> ConstraintSet<'db, 'c>;
        fn reduce_inferable(constraints: ConstraintSet<'db, 'c>, inferable: TypeVarSet<'db>) -> ConstraintSet<'db, 'c>;
        fn max_freshness(signature: &Signature<'db>, context: GenericContext<'db>) -> Option<TypeVarNonce>;
        fn freshen_signature(signature: &Signature<'db>, delta: u32) -> Signature<'db>;
        fn signature_typevars(signature: &Signature<'db>) -> TypeVarSet<'db>;
        fn aggregate_candidate(ty: Type<'db>) -> bool;
        fn union_add(builder: UnionBuilder<'db>, ty: Type<'db>) -> UnionBuilder<'db>;
        fn union_build(builder: UnionBuilder<'db>) -> Type<'db>;
        fn resolve_alias(ty: Type<'db>) -> Type<'db>;
        fn parameter_contains_typevar(parameters: &Parameters<'db>, typevar: BoundTypeVarInstance<'db>) -> bool;
        fn expand_parameters(parameters: &Parameters<'db>) -> Parameters<'db>;
        fn normalize_variadic_parameters(source: Parameters<'db>, target: Parameters<'db>) -> (Parameters<'db>, Parameters<'db>);
        fn empty_tuple() -> Type<'db>;
    }

    async fn tuple_from_parameters<'p>(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        parameters: impl Iterator<Item = &'p Parameter<'db>> + Clone,
    ) -> Result<Type<'db>, Captured>
    where
        'db: 'p,
    {
        LegacyInlineEffects
            .tuple_from_parameters(db, checker, parameters)
            .await
            .map_err(|never| match never {})
    }
}

#[test]
fn signature_effect_retains_derived_checker_after_unwinding() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/signature_view.py",
            r#"from typing import overload

def source[T](value: T) -> T: ...
def target(value: int) -> int: ...
def rejected(value: int) -> str: ...

@overload
def alternatives[T](value: T) -> T: ...
@overload
def alternatives(value: str) -> str: ...
def alternatives(value): ...
"#,
        )
        .build()?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/signature_view.py")?,
        env.program(&db),
    );
    let signature = |name| {
        let Type::FunctionLiteral(function) = global_symbol(&db, file, name).place.expect_type()
        else {
            anyhow::bail!("missing fixture function {name}");
        };
        Ok(function.signature(&db))
    };
    let target = signature("target")?;
    let rejected = signature("rejected")?;
    for (name, context_enabled) in [("source", true), ("alternatives", false)] {
        let source = signature(name)?;
        let constraints = ConstraintSetBuilder::new();
        let relation_visitor = HasRelationToVisitor::default(&constraints);
        let disjointness_visitor = IsDisjointVisitor::default(&constraints);
        let signature_visitor = SignatureRelationVisitor::default();
        let mapping_visitor = ApplyTypeMappingVisitor::new(&env);
        let root = TypeRelationChecker::constraint_set_assignability_with_context(
            &env,
            &constraints,
            &relation_visitor,
            &disjointness_visitor,
            &signature_visitor,
            &mapping_visitor,
        );
        let effects = CaptureRelation::default();
        assert!(matches!(
            try_poll_immediate(
                root.check_callable_signature_pair_with(&db, &effects, source, target,)
            ),
            Poll::Ready(Err(Captured))
        ));
        assert!(signature_visitor.is_empty());
        let retained = effects
            .retained
            .borrow_mut()
            .take()
            .ok_or_else(|| anyhow::anyhow!("the signature must reach its return comparison"))?;

        // The signature future and its local checker views have ended. The saved view still
        // carries signature-local inference and the caller's original constraint domain.
        let Type::TypeVar(variable) = retained.source else {
            anyhow::bail!("the first return comparison must retain the generic variable");
        };
        assert!(variable.is_inferable(&db, retained.checker.inferable));
        assert_eq!(root.inferable, TypeVarSet::None);
        assert!(std::ptr::eq(
            retained.checker.constraints,
            std::ptr::from_ref(&constraints)
        ));
        assert!(std::ptr::eq(
            retained.checker.signature_relation_visitor,
            std::ptr::from_ref(&signature_visitor),
        ));
        assert!(std::ptr::eq(
            retained.checker.materialization_visitor,
            std::ptr::from_ref(&mapping_visitor)
        ));
        assert_eq!(
            retained.checker.is_context_collection_enabled(),
            context_enabled
        );
        assert!(root.is_context_collection_enabled());
        assert!(!retained.live_result.is_trivially_always_satisfied());
        assert!(!retained.live_result.is_trivially_never_satisfied());
        let child = retained
            .checker
            .check_type_pair(&db, retained.source, retained.target);
        assert!(child.ownership_probe_same_set(retained.live_result));

        // A real failed signature writes through the retained view only when that view has
        // context collection enabled. The root keeps its own enabled flag in both cases.
        assert!(
            root.report_context()
                .is_some_and(ErrorContextTree::is_empty)
        );
        let failure = retained
            .checker
            .check_callable_signature_pair(&db, rejected, target);
        assert!(failure.is_never_satisfied(&db, &env));
        assert_eq!(
            root.report_context()
                .is_some_and(|context| !context.is_empty()),
            context_enabled,
        );
        assert!(root.is_context_collection_enabled());
        assert!(signature_visitor.is_empty());
    }
    Ok(())
}
