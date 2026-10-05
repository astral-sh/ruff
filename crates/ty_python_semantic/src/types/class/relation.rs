//! Nominal class comparison with lazy MRO and explicit-ancestor traversal.

use std::convert::Infallible;
use std::ops::ControlFlow;

use crate::Db;
use crate::types::class_base::ClassBase;
use crate::types::constraints::{ConstraintFold, ConstraintFoldKind, ConstraintSet};
use crate::types::mro::MroIterator;
use crate::types::relation::{TypeRelation, TypeRelationChecker};
use crate::types::{ClassLiteral, ClassType, GenericAlias};

use super::ExplicitClassAncestors;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousClassPairEffects)]
    pub(in crate::types) trait ClassPairEffects<'c, 'db: 'c> {
        type Error;
        type MroCursor<'state> where Self: 'state;
        type Ancestors<'state> where Self: 'state;
        type Fold<'state> where Self: 'state;

        #[operation(local)]
        async fn same_literal(&self, source: ClassLiteral<'db>, target: ClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn same_origin(&self, source: GenericAlias<'db>, target: GenericAlias<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn specialization_pair(&self, source: GenericAlias<'db>, target: GenericAlias<'db>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(local)]
        async fn constant(&self, value: bool) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(local)]
        async fn is_object(&self, class: ClassType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn is_final(&self, class: ClassType<'db>) -> Result<bool, Self::Error>;

        #[operation(child)]
        async fn mro_start(&self, class: ClassType<'db>) -> Result<Self::MroCursor<'_>, Self::Error>;
        #[operation(child)]
        #[progress]
        async fn mro_next<'state>(&'state self, cursor: &mut Self::MroCursor<'state>) -> Result<Option<ClassBase<'db>>, Self::Error>;
        #[operation(local)]
        async fn fold_start(&self) -> Result<Self::Fold<'_>, Self::Error>;
        #[operation(child)]
        async fn fold_push<'state>(&'state self, fold: &mut Self::Fold<'state>, next: ConstraintSet<'db, 'c>) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Self::Error>;
        #[operation(child)]
        async fn fold_finish<'state>(&'state self, fold: &mut Self::Fold<'state>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

        #[operation(local)]
        async fn is_always(&self, value: ConstraintSet<'db, 'c>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn ancestors_start(&self, class: ClassType<'db>) -> Result<Self::Ancestors<'_>, Self::Error>;
        #[operation(child)]
        #[progress]
        async fn ancestors_next<'state>(&'state self, cursor: &mut Self::Ancestors<'state>) -> Result<Option<ClassType<'db>>, Self::Error>;
        #[operation(child)]
        async fn disjoin(&self, left: ConstraintSet<'db, 'c>, right: ConstraintSet<'db, 'c>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
    }

    #[synchronous(check_class_pair_sync)]
    #[capabilities(effects = ClassPairEffects)]
    #[passive_values()]
    pub(in crate::types) async fn check_class_pair_with<'c, 'db: 'c, E: ClassPairEffects<'c, 'db>>(
        source: ClassType<'db>,
        target: ClassType<'db>,
        relation: TypeRelation,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> {
        // Fast path: if source and target are the same class (possibly with different
        // specializations), we can compare them directly without walking the MRO.
        match (source, target) {
            (ClassType::NonGeneric(source), ClassType::NonGeneric(target))
                if effects.same_literal(source, target).await? =>
            {
                return effects.constant(true).await;
            }
            (ClassType::Generic(source), ClassType::Generic(target))
                if effects.same_origin(source, target).await? =>
            {
                return effects.specialization_pair(source, target).await;
            }
            _ => {}
        }

        let (result, generic_target) = {
            let mut mro = effects.mro_start(source).await?;
            let mut fold = effects.fold_start().await?;
            #[passive_state]
            let mut generic_target = None;
            #[passive_state]
            let mut saturated = None;
            #[cursor_loop]
            while let Some(base) = effects.mro_next(&mut mro).await? {
                let next = match base {
                    ClassBase::Any => {
                        let value = matches!(relation, TypeRelation::Assignability)
                            || effects.is_object(target).await?;
                        effects.constant(value).await?
                    }
                    ClassBase::Dynamic(_) | ClassBase::Divergent(_) => {
                        let value = match relation {
                            TypeRelation::Subtyping
                            | TypeRelation::Redundancy { .. }
                            | TypeRelation::SubtypingAssuming => effects.is_object(target).await?,
                            TypeRelation::Assignability => !effects.is_final(target).await?,
                        };
                        effects.constant(value).await?
                    }

                    // Protocol, Generic, and TypedDict are special bases that don't match ClassType.
                    ClassBase::Protocol | ClassBase::Generic | ClassBase::TypedDict(_) => {
                        effects.constant(false).await?
                    }

                    ClassBase::Class(source) => match (source, target) {
                        // Two non-generic classes match if they have the same class literal.
                        (ClassType::NonGeneric(source), ClassType::NonGeneric(target)) => {
                            let equal = effects.same_literal(source, target).await?;
                            effects.constant(equal).await?
                        }

                        // Two generic classes match if they have the same origin and compatible specializations.
                        (ClassType::Generic(source), ClassType::Generic(target))
                            if effects.same_origin(source, target).await? =>
                        {
                            generic_target = Some(target);
                            effects.specialization_pair(source, target).await?
                        }

                        // Different generic origins, or generic and non-generic classes, don't match.
                        (ClassType::Generic(_), ClassType::Generic(_) | ClassType::NonGeneric(_))
                        | (ClassType::NonGeneric(_), ClassType::Generic(_)) => {
                            effects.constant(false).await?
                        }
                    },
                };
                if let ControlFlow::Break(result) = effects.fold_push(&mut fold, next).await? {
                    saturated = Some(result);
                    break;
                }
            }
            let result = match saturated {
                Some(result) => result,
                None => effects.fold_finish(&mut fold).await?,
            };
            (result, generic_target)
        };

        let Some(target) = generic_target else {
            return Ok(result);
        };
        if effects.is_always(result).await? {
            return Ok(result);
        }

        // The MRO retains only one specialization per class. A different inheritance path can
        // still establish the relation: `Child(Gradual, Concrete)` is a subtype of `Base[int]`
        // through `Concrete`, even if `Gradual` contributes `Base[Any]` to Child's MRO.
        let alternatives = {
            let mut ancestors = effects.ancestors_start(source).await?;
            let mut fold = effects.fold_start().await?;
            #[passive_state]
            let mut saturated = None;
            #[cursor_loop]
            while let Some(ancestor) = effects.ancestors_next(&mut ancestors).await? {
                let ClassType::Generic(ancestor) = ancestor else {
                    continue;
                };
                if !effects.same_origin(ancestor, target).await? {
                    continue;
                }
                let next = effects.specialization_pair(ancestor, target).await?;
                if let ControlFlow::Break(result) = effects.fold_push(&mut fold, next).await? {
                    saturated = Some(result);
                    break;
                }
            }
            match saturated {
                Some(result) => result,
                None => effects.fold_finish(&mut fold).await?,
            }
        };
        effects.disjoin(result, alternatives).await
    }
}

pub(super) struct InlineClassPairEffects<'db, 'check, 'a, 'c> {
    db: &'db dyn Db,
    checker: &'check TypeRelationChecker<'a, 'c, 'db>,
}

impl<'db, 'check, 'a, 'c> InlineClassPairEffects<'db, 'check, 'a, 'c> {
    pub(super) fn new(db: &'db dyn Db, checker: &'check TypeRelationChecker<'a, 'c, 'db>) -> Self {
        Self { db, checker }
    }
}

impl<'c, 'db: 'c> SynchronousClassPairEffects<'c, 'db> for InlineClassPairEffects<'db, '_, '_, 'c> {
    type Error = Infallible;
    type MroCursor<'state>
        = MroIterator<'db>
    where
        Self: 'state;
    type Ancestors<'state>
        = ExplicitClassAncestors<'state, 'db>
    where
        Self: 'state;
    type Fold<'state>
        = ConstraintFold<'db, 'c>
    where
        Self: 'state;

    fn same_literal(
        &self,
        source: ClassLiteral<'db>,
        target: ClassLiteral<'db>,
    ) -> Result<bool, Infallible> {
        Ok(source == target)
    }

    fn same_origin(
        &self,
        source: GenericAlias<'db>,
        target: GenericAlias<'db>,
    ) -> Result<bool, Infallible> {
        Ok(source.origin(self.db) == target.origin(self.db))
    }

    fn specialization_pair(
        &self,
        source: GenericAlias<'db>,
        target: GenericAlias<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(self.checker.check_specialization_pair(
            self.db,
            source.specialization(self.db),
            target.specialization(self.db),
        ))
    }

    fn constant(&self, value: bool) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(ConstraintSet::from_bool(self.checker.constraints, value))
    }

    fn is_object(&self, class: ClassType<'db>) -> Result<bool, Infallible> {
        Ok(class.is_object(self.db))
    }

    fn is_final(&self, class: ClassType<'db>) -> Result<bool, Infallible> {
        Ok(class.is_final(self.db))
    }

    fn mro_start(&self, class: ClassType<'db>) -> Result<Self::MroCursor<'_>, Infallible> {
        Ok(class.iter_mro(self.db))
    }

    fn mro_next<'state>(
        &'state self,
        cursor: &mut Self::MroCursor<'state>,
    ) -> Result<Option<ClassBase<'db>>, Infallible> {
        Ok(cursor.next())
    }

    fn fold_start(&self) -> Result<Self::Fold<'_>, Infallible> {
        Ok(ConstraintFold::new(
            self.checker.constraints,
            ConstraintFoldKind::Any,
        ))
    }

    fn fold_push<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
        next: ConstraintSet<'db, 'c>,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Infallible> {
        Ok(fold.push(next))
    }

    fn fold_finish<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(fold.finish_borrowed())
    }

    fn is_always(&self, value: ConstraintSet<'db, 'c>) -> Result<bool, Infallible> {
        value.verify_builder(self.checker.constraints);
        Ok(value.is_trivially_always_satisfied())
    }

    fn ancestors_start(&self, class: ClassType<'db>) -> Result<Self::Ancestors<'_>, Infallible> {
        Ok(class.iter_explicit_ancestors(self.db, self.checker.env))
    }

    fn ancestors_next<'state>(
        &'state self,
        cursor: &mut Self::Ancestors<'state>,
    ) -> Result<Option<ClassType<'db>>, Infallible> {
        Ok(cursor.next())
    }

    fn disjoin(
        &self,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(left.or(self.db, self.checker.constraints, || right))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};

    use ruff_db::files::system_path_to_file;
    use ruff_python_ast::PythonVersion;
    use salsa::plumbing::AsId;
    use ty_python_core::ProgramFile;

    use super::*;
    use crate::db::tests::{TestDb, TestDbBuilder};
    use crate::place::global_symbol;
    use crate::types::constraints::{ConstraintSetBuilder, IteratorConstraintsExtension};
    use crate::types::relation::{HasRelationToVisitor, IsDisjointVisitor, TypeVarEvaluation};
    use crate::types::signatures::SignatureRelationVisitor;
    use crate::types::typevar::TypeVarSet;
    use crate::types::{ApplyTypeMappingVisitor, DynamicType, Type, TypingModule};

    fn database() -> anyhow::Result<TestDb> {
        TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .with_file(
                "/src/class_pair.py",
                r#"
from builtins import object as Object
from typing import Any, final

class Plain: ...
class Other: ...
@final
class Final: ...
Dynamic = type("Dynamic", (), {})

class Base[T]:
    value: T

class OtherBase[T]:
    value: T

class Gradual(Base[Any]): ...
class Concrete(Base[int]): ...
class Child(Gradual, Concrete): ...
class Reversed(Concrete, Gradual): ...

class Left(Base[int]): ...
class Right(Base[str]): ...
class Alternatives(Left, Right): ...
class ReversedAlternatives(Right, Left): ...

IntBase = Base[int]
StrBase = Base[str]
OtherIntBase = OtherBase[int]
"#,
            )
            .build()
    }

    fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<ClassType<'db>> {
        let env = db.program_environment();
        let file = ProgramFile::new(
            db,
            system_path_to_file(db, "/src/class_pair.py")?,
            env.program(db),
        );
        match global_symbol(db, file, name).place.expect_type() {
            Type::ClassLiteral(class) => Ok(class.identity_specialization(db)),
            Type::GenericAlias(alias) => Ok(ClassType::Generic(alias)),
            value => anyhow::bail!("expected class {name}, got {value:?}"),
        }
    }

    fn with_checker<'db>(
        db: &'db TestDb,
        relation: TypeRelation,
        inferable: TypeVarSet<'db>,
        check: impl FnOnce(&TypeRelationChecker<'_, '_, 'db>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let env = db.program_environment();
        let constraints = ConstraintSetBuilder::new();
        let relations = HasRelationToVisitor::default(&constraints);
        let disjointness = IsDisjointVisitor::default(&constraints);
        let signatures = SignatureRelationVisitor::default();
        let mapping = ApplyTypeMappingVisitor::new(&env);
        let checker = TypeRelationChecker::new(
            &env,
            relation,
            &constraints,
            inferable,
            &relations,
            &disjointness,
            &signatures,
            &mapping,
        );
        check(&checker)
    }

    fn infallible<T>(result: Result<T, Infallible>) -> T {
        match result {
            Ok(value) => value,
            Err(never) => match never {},
        }
    }

    fn ready<T>(future: impl Future<Output = T>) -> anyhow::Result<T> {
        match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
            Poll::Ready(value) => Ok(value),
            Poll::Pending => anyhow::bail!("recording effects unexpectedly suspended"),
        }
    }

    // Keep the pre-extraction decisions and iterator folds independent of the shared driver.
    // Comparing full constraint identities also checks the grouping of source-order histories.
    fn original<'c, 'db>(
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'_, 'c, 'db>,
        source: ClassType<'db>,
        target: ClassType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        match (source, target) {
            (ClassType::NonGeneric(source), ClassType::NonGeneric(target)) if source == target => {
                return checker.always();
            }
            (ClassType::Generic(source), ClassType::Generic(target))
                if source.origin(db) == target.origin(db) =>
            {
                return checker.check_specialization_pair(
                    db,
                    source.specialization(db),
                    target.specialization(db),
                );
            }
            _ => {}
        }
        let mut generic_target = None;
        let result = source
            .iter_mro(db)
            .when_any(db, checker.constraints, |base| match base {
                ClassBase::Any => ConstraintSet::from_bool(
                    checker.constraints,
                    checker.relation.is_assignability() || target.is_object(db),
                ),
                ClassBase::Dynamic(_) | ClassBase::Divergent(_) => ConstraintSet::from_bool(
                    checker.constraints,
                    match checker.relation {
                        TypeRelation::Subtyping
                        | TypeRelation::Redundancy { .. }
                        | TypeRelation::SubtypingAssuming => target.is_object(db),
                        TypeRelation::Assignability => !target.is_final(db),
                    },
                ),
                ClassBase::Protocol | ClassBase::Generic | ClassBase::TypedDict(_) => {
                    checker.never()
                }
                ClassBase::Class(source) => match (source, target) {
                    (ClassType::NonGeneric(source), ClassType::NonGeneric(target)) => {
                        ConstraintSet::from_bool(checker.constraints, source == target)
                    }
                    (ClassType::Generic(source), ClassType::Generic(target))
                        if source.origin(db) == target.origin(db) =>
                    {
                        generic_target = Some(target);
                        checker.check_specialization_pair(
                            db,
                            source.specialization(db),
                            target.specialization(db),
                        )
                    }
                    (ClassType::Generic(_), ClassType::Generic(_) | ClassType::NonGeneric(_))
                    | (ClassType::NonGeneric(_), ClassType::Generic(_)) => checker.never(),
                },
            });
        let Some(target) = generic_target else {
            return result;
        };
        result.or(db, checker.constraints, || {
            source
                .iter_explicit_ancestors(db, checker.env)
                .filter_map(ClassType::into_generic_alias)
                .filter(|ancestor| ancestor.origin(db) == target.origin(db))
                .when_any(db, checker.constraints, |ancestor| {
                    checker.check_specialization_pair(
                        db,
                        ancestor.specialization(db),
                        target.specialization(db),
                    )
                })
        })
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Event<'db> {
        SameLiteral,
        SameOrigin,
        Specialization(GenericAlias<'db>, GenericAlias<'db>),
        Constant(bool),
        IsObject,
        IsFinal,
        MroStart,
        MroNext,
        FoldStart,
        FoldPush,
        FoldFinish,
        IsAlways,
        AncestorsStart,
        AncestorsNext,
        Disjoin,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Refused(usize);

    enum MroCursor<'state, 'db> {
        Native(MroIterator<'db>),
        Specified(std::slice::Iter<'state, ClassBase<'db>>),
    }

    struct Recording<'db, 'check, 'a, 'c> {
        inline: InlineClassPairEffects<'db, 'check, 'a, 'c>,
        events: RefCell<Vec<Event<'db>>>,
        refuse_at: Option<usize>,
        specified_mro: Option<Vec<ClassBase<'db>>>,
    }

    impl<'db, 'check, 'a, 'c> Recording<'db, 'check, 'a, 'c> {
        fn new(db: &'db dyn Db, checker: &'check TypeRelationChecker<'a, 'c, 'db>) -> Self {
            Self {
                inline: InlineClassPairEffects::new(db, checker),
                events: RefCell::default(),
                refuse_at: None,
                specified_mro: None,
            }
        }

        fn record(&self, event: Event<'db>) -> Result<(), Refused> {
            let mut events = self.events.borrow_mut();
            let index = events.len();
            events.push(event);
            if self.refuse_at == Some(index) {
                Err(Refused(index))
            } else {
                Ok(())
            }
        }
    }

    macro_rules! recording_methods {
        ($(fn $name:ident($($argument:ident: $argument_ty:ty),* $(,)?) -> $result:ty => $event:expr;)*) => {
            $(fn $name(&self, $($argument: $argument_ty),*) -> Result<$result, Refused> {
                self.record($event)?;
                Ok(infallible(self.inline.$name($($argument),*)))
            })*
        };
    }

    impl<'c, 'db: 'c> SynchronousClassPairEffects<'c, 'db> for Recording<'db, '_, '_, 'c> {
        type Error = Refused;
        type MroCursor<'state>
            = MroCursor<'state, 'db>
        where
            Self: 'state;
        type Ancestors<'state>
            = ExplicitClassAncestors<'state, 'db>
        where
            Self: 'state;
        type Fold<'state>
            = ConstraintFold<'db, 'c>
        where
            Self: 'state;

        recording_methods! {
            fn same_literal(source: ClassLiteral<'db>, target: ClassLiteral<'db>) -> bool => Event::SameLiteral;
            fn same_origin(source: GenericAlias<'db>, target: GenericAlias<'db>) -> bool => Event::SameOrigin;
            fn specialization_pair(source: GenericAlias<'db>, target: GenericAlias<'db>) -> ConstraintSet<'db, 'c> => Event::Specialization(source, target);
            fn constant(value: bool) -> ConstraintSet<'db, 'c> => Event::Constant(value);
            fn is_object(class: ClassType<'db>) -> bool => Event::IsObject;
            fn is_final(class: ClassType<'db>) -> bool => Event::IsFinal;
            fn fold_start() -> Self::Fold<'_> => Event::FoldStart;
            fn is_always(value: ConstraintSet<'db, 'c>) -> bool => Event::IsAlways;
            fn ancestors_start(class: ClassType<'db>) -> Self::Ancestors<'_> => Event::AncestorsStart;
            fn disjoin(left: ConstraintSet<'db, 'c>, right: ConstraintSet<'db, 'c>) -> ConstraintSet<'db, 'c> => Event::Disjoin;
        }

        fn mro_start(&self, class: ClassType<'db>) -> Result<Self::MroCursor<'_>, Refused> {
            self.record(Event::MroStart)?;
            Ok(match &self.specified_mro {
                Some(bases) => MroCursor::Specified(bases.iter()),
                None => MroCursor::Native(infallible(self.inline.mro_start(class))),
            })
        }

        fn mro_next<'state>(
            &'state self,
            cursor: &mut Self::MroCursor<'state>,
        ) -> Result<Option<ClassBase<'db>>, Refused> {
            self.record(Event::MroNext)?;
            Ok(match cursor {
                MroCursor::Native(cursor) => infallible(self.inline.mro_next(cursor)),
                MroCursor::Specified(cursor) => cursor.next().copied(),
            })
        }

        fn fold_push<'state>(
            &'state self,
            fold: &mut Self::Fold<'state>,
            next: ConstraintSet<'db, 'c>,
        ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Refused> {
            self.record(Event::FoldPush)?;
            Ok(infallible(self.inline.fold_push(fold, next)))
        }

        fn fold_finish<'state>(
            &'state self,
            fold: &mut Self::Fold<'state>,
        ) -> Result<ConstraintSet<'db, 'c>, Refused> {
            self.record(Event::FoldFinish)?;
            Ok(infallible(self.inline.fold_finish(fold)))
        }

        fn ancestors_next<'state>(
            &'state self,
            cursor: &mut Self::Ancestors<'state>,
        ) -> Result<Option<ClassType<'db>>, Refused> {
            self.record(Event::AncestorsNext)?;
            Ok(infallible(self.inline.ancestors_next(cursor)))
        }
    }

    macro_rules! async_recording_methods {
        ($(fn $name:ident($($argument:ident: $argument_ty:ty),* $(,)?) -> $result:ty;)*) => {
            $(async fn $name(&self, $($argument: $argument_ty),*) -> Result<$result, Refused> {
                SynchronousClassPairEffects::$name(self, $($argument),*)
            })*
        };
    }

    impl<'c, 'db: 'c> ClassPairEffects<'c, 'db> for Recording<'db, '_, '_, 'c> {
        type Error = Refused;
        type MroCursor<'state>
            = MroCursor<'state, 'db>
        where
            Self: 'state;
        type Ancestors<'state>
            = ExplicitClassAncestors<'state, 'db>
        where
            Self: 'state;
        type Fold<'state>
            = ConstraintFold<'db, 'c>
        where
            Self: 'state;

        async_recording_methods! {
            fn same_literal(source: ClassLiteral<'db>, target: ClassLiteral<'db>) -> bool;
            fn same_origin(source: GenericAlias<'db>, target: GenericAlias<'db>) -> bool;
            fn specialization_pair(source: GenericAlias<'db>, target: GenericAlias<'db>) -> ConstraintSet<'db, 'c>;
            fn constant(value: bool) -> ConstraintSet<'db, 'c>;
            fn is_object(class: ClassType<'db>) -> bool;
            fn is_final(class: ClassType<'db>) -> bool;
            fn mro_start(class: ClassType<'db>) -> Self::MroCursor<'_>;
            fn fold_start() -> Self::Fold<'_>;
            fn is_always(value: ConstraintSet<'db, 'c>) -> bool;
            fn ancestors_start(class: ClassType<'db>) -> Self::Ancestors<'_>;
            fn disjoin(left: ConstraintSet<'db, 'c>, right: ConstraintSet<'db, 'c>) -> ConstraintSet<'db, 'c>;
        }

        async fn mro_next<'state>(
            &'state self,
            cursor: &mut Self::MroCursor<'state>,
        ) -> Result<Option<ClassBase<'db>>, Refused> {
            SynchronousClassPairEffects::mro_next(self, cursor)
        }

        async fn fold_push<'state>(
            &'state self,
            fold: &mut Self::Fold<'state>,
            next: ConstraintSet<'db, 'c>,
        ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Refused> {
            SynchronousClassPairEffects::fold_push(self, fold, next)
        }

        async fn fold_finish<'state>(
            &'state self,
            fold: &mut Self::Fold<'state>,
        ) -> Result<ConstraintSet<'db, 'c>, Refused> {
            SynchronousClassPairEffects::fold_finish(self, fold)
        }

        async fn ancestors_next<'state>(
            &'state self,
            cursor: &mut Self::Ancestors<'state>,
        ) -> Result<Option<ClassType<'db>>, Refused> {
            SynchronousClassPairEffects::ancestors_next(self, cursor)
        }
    }

    fn completed<T>(result: Result<T, Refused>) -> anyhow::Result<T> {
        result.map_err(|error| anyhow::anyhow!("unexpected refusal at {error:?}"))
    }

    #[test]
    fn identity_and_generic_origin_skip_mro() -> anyhow::Result<()> {
        let db = database()?;
        with_checker(&db, TypeRelation::Subtyping, TypeVarSet::None, |checker| {
            for name in ["Plain", "Dynamic"] {
                let class = class(&db, name)?;
                let effects = Recording::new(&db, checker);
                let result = completed(check_class_pair_sync(
                    class,
                    class,
                    checker.relation,
                    &effects,
                ))?;
                assert!(result.is_trivially_always_satisfied());
                assert_eq!(
                    *effects.events.borrow(),
                    [Event::SameLiteral, Event::Constant(true)]
                );
            }
            let source = class(&db, "IntBase")?;
            for name in ["IntBase", "StrBase"] {
                let target = class(&db, name)?;
                let (ClassType::Generic(source_alias), ClassType::Generic(target_alias)) =
                    (source, target)
                else {
                    anyhow::bail!("expected generic aliases");
                };
                let effects = Recording::new(&db, checker);
                let result = completed(check_class_pair_sync(
                    source,
                    target,
                    checker.relation,
                    &effects,
                ))?;
                assert!(result.ownership_probe_same_set(original(&db, checker, source, target)));
                assert_eq!(
                    *effects.events.borrow(),
                    [
                        Event::SameOrigin,
                        Event::Specialization(source_alias, target_alias)
                    ]
                );
            }
            Ok(())
        })
    }

    #[test]
    fn gradual_bases_preserve_relation_and_finality_gates() -> anyhow::Result<()> {
        let db = database()?;
        let source = class(&db, "Plain")?;
        let literal = source
            .class_literal(&db)
            .as_static()
            .ok_or_else(|| anyhow::anyhow!("missing static class"))?;
        let Type::Divergent(divergent) = Type::divergent(literal.as_id()) else {
            anyhow::bail!("expected divergent marker");
        };
        for relation in [
            TypeRelation::Subtyping,
            TypeRelation::SubtypingAssuming,
            TypeRelation::Redundancy { pure: false },
            TypeRelation::Redundancy { pure: true },
            TypeRelation::Assignability,
        ] {
            with_checker(&db, relation, TypeVarSet::None, |checker| {
                for base in [
                    ClassBase::Any,
                    ClassBase::Dynamic(DynamicType::Any),
                    ClassBase::unknown(),
                    ClassBase::Divergent(divergent),
                ] {
                    for name in ["Object", "Final", "Other"] {
                        let target = class(&db, name)?;
                        let mut effects = Recording::new(&db, checker);
                        effects.specified_mro = Some(vec![base]);
                        let result =
                            completed(check_class_pair_sync(source, target, relation, &effects))?;
                        let assignable = matches!(relation, TypeRelation::Assignability);
                        let explicit_any = matches!(base, ClassBase::Any);
                        let expected = if assignable {
                            explicit_any || name != "Final"
                        } else {
                            name == "Object"
                        };
                        assert_eq!(
                            result.is_trivially_always_satisfied(),
                            expected,
                            "{base:?}, {relation:?}, {name}"
                        );
                        let events = effects.events.borrow();
                        assert_eq!(
                            events.contains(&Event::IsFinal),
                            assignable && !explicit_any
                        );
                        assert_eq!(events.contains(&Event::IsObject), !assignable);
                        assert!(!events.contains(&Event::AncestorsStart));
                    }
                }
                Ok(())
            })?;
        }
        Ok(())
    }

    #[test]
    fn mro_filtering_and_saturation_remain_lazy() -> anyhow::Result<()> {
        let db = database()?;
        with_checker(&db, TypeRelation::Subtyping, TypeVarSet::None, |checker| {
            let source = class(&db, "Plain")?;
            let target = class(&db, "IntBase")?;
            let mut effects = Recording::new(&db, checker);
            effects.specified_mro = Some(vec![
                ClassBase::Protocol,
                ClassBase::Generic,
                ClassBase::TypedDict(TypingModule::Typing),
                ClassBase::Class(source),
                ClassBase::Class(class(&db, "OtherIntBase")?),
                ClassBase::Class(target),
                ClassBase::unknown(),
            ]);
            let result = completed(check_class_pair_sync(
                source,
                target,
                checker.relation,
                &effects,
            ))?;
            assert!(result.is_trivially_always_satisfied());
            let events = effects.events.borrow();
            assert_eq!(
                events
                    .iter()
                    .filter(|event| **event == Event::MroNext)
                    .count(),
                6
            );
            assert_eq!(
                events
                    .iter()
                    .filter(|event| matches!(event, Event::Specialization(..)))
                    .count(),
                1
            );
            assert!(!events.contains(&Event::FoldFinish));
            assert!(!events.contains(&Event::AncestorsStart));
            assert!(!events.contains(&Event::IsObject));

            let mut effects = Recording::new(&db, checker);
            effects.specified_mro = Some(vec![ClassBase::Any, ClassBase::Class(target)]);
            let result = completed(check_class_pair_sync(
                source,
                target,
                TypeRelation::Assignability,
                &effects,
            ))?;
            assert!(result.is_trivially_always_satisfied());
            assert!(!effects.events.borrow().contains(&Event::IsAlways));
            assert!(
                !effects
                    .events
                    .borrow()
                    .iter()
                    .any(|event| matches!(event, Event::Specialization(..)))
            );
            Ok(())
        })
    }

    #[test]
    fn alternatives_follow_mro_exhaustion_and_specialization_order() -> anyhow::Result<()> {
        let db = database()?;
        with_checker(&db, TypeRelation::Subtyping, TypeVarSet::None, |checker| {
            let source = class(&db, "Child")?;
            let target = class(&db, "IntBase")?;
            let effects = Recording::new(&db, checker);
            let result = completed(check_class_pair_sync(
                source,
                target,
                checker.relation,
                &effects,
            ))?;
            assert!(result.is_trivially_always_satisfied());
            assert!(result.ownership_probe_same_set(original(&db, checker, source, target)));
            let events = effects.events.borrow();
            let ancestor_start = events
                .iter()
                .position(|event| *event == Event::AncestorsStart)
                .ok_or_else(|| anyhow::anyhow!("missing alternative search"))?;
            assert_eq!(
                &events[ancestor_start - 3..ancestor_start],
                [Event::MroNext, Event::FoldFinish, Event::IsAlways]
            );
            let specializations: Vec<_> = events
                .iter()
                .filter_map(|event| match event {
                    Event::Specialization(source, _) => Some(*source),
                    _ => None,
                })
                .collect();
            assert_eq!(specializations.len(), 3);
            assert_eq!(specializations[0], specializations[1]);
            assert_eq!(ClassType::Generic(specializations[2]), target);
            assert_eq!(events.last(), Some(&Event::Disjoin));

            for (source_name, target_name) in [("Reversed", "IntBase"), ("Child", "OtherIntBase")] {
                let effects = Recording::new(&db, checker);
                completed(check_class_pair_sync(
                    class(&db, source_name)?,
                    class(&db, target_name)?,
                    checker.relation,
                    &effects,
                ))?;
                assert!(!effects.events.borrow().contains(&Event::AncestorsStart));
            }
            Ok(())
        })
    }

    #[test]
    fn shared_forms_preserve_nonterminal_constraints_and_source_order() -> anyhow::Result<()> {
        let db = database()?;
        let target = class(&db, "Base")?;
        let context = target
            .class_literal(&db)
            .generic_context(&db)
            .ok_or_else(|| anyhow::anyhow!("missing generic context"))?;
        let inferable = TypeVarSet::from_typevars(&db, context.variables(&db));
        with_checker(&db, TypeRelation::Subtyping, inferable, |checker| {
            // Lazy evaluation retains each inheritance path as a constraint on T.
            let mut checker = checker.clone();
            checker.typevar_evaluation = TypeVarEvaluation::Lazy;
            let checker = &checker;
            let source = class(&db, "Alternatives")?;
            let expected = original(&db, checker, source, target);
            assert!(!expected.is_trivially_always_satisfied());
            assert!(!expected.is_trivially_never_satisfied());
            assert!(
                context
                    .variables(&db)
                    .all(|variable| expected.mentions_typevar(&db, variable))
            );
            let synchronous = Recording::new(&db, checker);
            let actual = completed(check_class_pair_sync(
                source,
                target,
                checker.relation,
                &synchronous,
            ))?;
            assert!(actual.ownership_probe_same_set(expected));
            let asynchronous = Recording::new(&db, checker);
            let actual = completed(ready(check_class_pair_with(
                source,
                target,
                checker.relation,
                &asynchronous,
            ))?)?;
            assert!(actual.ownership_probe_same_set(expected));
            assert_eq!(*asynchronous.events.borrow(), *synchronous.events.borrow());
            let reversed = original(&db, checker, class(&db, "ReversedAlternatives")?, target);
            assert!(!expected.ownership_probe_same_set(reversed));
            Ok(())
        })
    }

    #[test]
    fn every_reached_effect_refuses_without_returning_a_constraint() -> anyhow::Result<()> {
        let db = database()?;
        let target = class(&db, "Base")?;
        let context = target
            .class_literal(&db)
            .generic_context(&db)
            .ok_or_else(|| anyhow::anyhow!("missing generic context"))?;
        let inferable = TypeVarSet::from_typevars(&db, context.variables(&db));
        with_checker(&db, TypeRelation::Subtyping, inferable, |checker| {
            let mut checker = checker.clone();
            checker.typevar_evaluation = TypeVarEvaluation::Lazy;
            let checker = &checker;
            let source = class(&db, "Alternatives")?;
            let complete = Recording::new(&db, checker);
            let expected = completed(check_class_pair_sync(
                source,
                target,
                checker.relation,
                &complete,
            ))?;
            assert!(!expected.is_trivially_always_satisfied());
            assert!(!expected.is_trivially_never_satisfied());
            assert!(
                context
                    .variables(&db)
                    .all(|variable| expected.mentions_typevar(&db, variable))
            );
            let events = complete.events.into_inner();
            assert!(events.contains(&Event::AncestorsStart));
            assert_eq!(events.last(), Some(&Event::Disjoin));
            for index in 0..events.len() {
                let mut effects = Recording::new(&db, checker);
                effects.refuse_at = Some(index);
                let result = check_class_pair_sync(source, target, checker.relation, &effects);
                assert!(matches!(result, Err(Refused(actual)) if actual == index));
                assert_eq!(*effects.events.borrow(), events[..=index]);
                let mut effects = Recording::new(&db, checker);
                effects.refuse_at = Some(index);
                let result = ready(check_class_pair_with(
                    source,
                    target,
                    checker.relation,
                    &effects,
                ))?;
                assert!(matches!(result, Err(Refused(actual)) if actual == index));
                assert_eq!(*effects.events.borrow(), events[..=index]);
                let effects = Recording::new(&db, checker);
                let retried = completed(check_class_pair_sync(
                    source,
                    target,
                    checker.relation,
                    &effects,
                ))?;
                assert!(retried.ownership_probe_same_set(expected));
            }
            Ok(())
        })
    }
}
