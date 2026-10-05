//! Shared type negation, with interning and normalization supplied by the caller.

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use super::{
    DivergentType, IntersectionBuilder, IntersectionType, NegativeIntersectionElements,
    NominalInstanceType, RecursiveType, Type,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

pub(in crate::types) struct NegationFacts;

shared_semantic_family! {
    #[synchronous(SynchronousNegationEffects)]
    pub(in crate::types) trait NegationEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn unbound_recursive(&self) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn unfold(&self, recursive: RecursiveType<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn recurse(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn single_negative(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn normalize(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl NegationFacts {
        fn object<'db>(&self) -> Type<'db> { Type::object() }
        fn is_object(&self, instance: NominalInstanceType<'_>) -> bool { instance.is_object() }
        pub(super) fn negated_divergent<'db>(&self, divergent: DivergentType) -> Type<'db> {
            match divergent.materialization_kind() {
                Some(materialization_kind) => {
                    Type::Divergent(divergent.materialized(materialization_kind.flip()))
                }
                None => Type::Divergent(divergent),
            }
        }
    }

    #[synchronous(negate_sync)]
    #[capabilities(effects = NegationEffects, facts = NegationFacts)]
    #[passive_values(Type::Never)]
    pub(in crate::types) async fn negate_with<'db, E: NegationEffects<'db>>(
        ty: Type<'db>,
        facts: NegationFacts,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        effects.checkpoint().await?;
        // Avoid invoking the `IntersectionBuilder` for negations that are trivial.
        //
        // We verify that this always produces the same result as
        // `IntersectionBuilder::new(db, env).add_negative(ty).build()` via the
        // property test `all_negated_types_identical_to_intersection_with_single_negated_element`
        Ok(match ty {
            Type::RecursiveVar(_) => effects.unbound_recursive().await?,
            Type::Never => facts.object(),
            Type::Dynamic(_) => ty,
            Type::Divergent(divergent) => facts.negated_divergent(divergent),
            Type::Recursive(recursive) => match effects.unfold(recursive).await? {
                Some(unfolded) => effects.recurse(unfolded).await?,
                None => effects.single_negative(ty).await?,
            },
            Type::NominalInstance(instance) if facts.is_object(instance) => Type::Never,
            Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::KnownBoundMethod(_)
            | Type::KnownInstance(_)
            | Type::SpecialForm(_)
            | Type::BoundSuper(_)
            | Type::FunctionLiteral(_)
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypeForm(_)
            | Type::TypeVar(_)
            | Type::TypedDict(_)
            | Type::NewTypeInstance(_)
            | Type::NominalInstance(_)
            | Type::ProtocolInstance(_)
            | Type::ModuleLiteral(_)
            | Type::ClassLiteral(_)
            | Type::GenericAlias(_)
            | Type::SubclassOf(_)
            | Type::PropertyInstance(_)
            | Type::SlotDescriptor(_)
            | Type::LiteralValue(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::Callable(_)
            | Type::WrapperDescriptor(_)
            | Type::TypeAlias(_)
            | Type::BoundMethod(_) => effects.single_negative(ty).await?,
            Type::Union(_) | Type::Intersection(_) | Type::EnumComplement(_) => {
                effects.normalize(ty).await?
            }
        })
    }
}

pub(in crate::types) struct OrdinaryNegationEffects<'env, 'db> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) env: &'env ProgramEnvironment<'db>,
}

impl<'db> SynchronousNegationEffects<'db> for OrdinaryNegationEffects<'_, 'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn unbound_recursive(&self) -> Result<Type<'db>, Self::Error> {
        unreachable!("semantic operation on an unbound recursive variable")
    }

    fn unfold(&self, recursive: RecursiveType<'db>) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(recursive.unfold(self.db, self.env).into_unfolded())
    }

    fn recurse(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.negate(self.db, self.env))
    }

    fn single_negative(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(Type::Intersection(IntersectionType::new(
            self.db,
            FxOrderSet::default(),
            NegativeIntersectionElements::Single(ty),
        )))
    }

    fn normalize(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(IntersectionBuilder::new(self.db, self.env)
            .add_negative(ty)
            .build())
    }
}
