//! Preserves callable binding behavior when dunder members are exposed through class lookup.

use std::convert::Infallible;
use std::slice;

use crate::types::callable::CallableTypeKind;
use crate::types::set_theoretic::builder::intersection_insertion::Elements;
use crate::types::set_theoretic::{IntersectionBuilder, RecursivelyDefined, UnionBuilder};
use crate::types::signatures::{CallableSignature, Signature};
use crate::types::{CallableType, IntersectionType, Type, UnionType};
use crate::{Db, ProgramEnvironment};

/// Selects the binding behavior applied to regular callable dunder members.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum DunderCallableTransform {
    /// Protects bare ParamSpec arguments from receiver binding after specialization.
    DunderParamSpec,
    /// A class member with parameters can bind its first parameter as a receiver.
    FunctionLike,
}

/// Type dispatch shared by [`super::own_member::into_dunder_paramspec_callable`] and
/// [`super::member_lookup::into_function_like_callable`]. Both inspect `Callable`, `Union`, and
/// `Intersection`; other types remain unchanged.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum DunderCallableMapping<'db> {
    Callable(CallableType<'db>),
    Union(UnionType<'db>),
    Intersection(IntersectionType<'db>),
    Unchanged,
}

/// Finite type classification and cursor setup used by both traversal providers.
#[derive(Debug)]
pub(in crate::types) struct DunderCallableFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousDunderCallableEffects)]
    pub(in crate::types) trait DunderCallableEffects<'db> {
        type Error;
        type Union;
        type Intersection;

        #[operation(local)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn callable_kind(&self, callable: CallableType<'db>) -> Result<CallableTypeKind, Self::Error>;
        #[operation(source)]
        async fn signatures(&self, callable: CallableType<'db>) -> Result<&'db CallableSignature<'db>, Self::Error>;
        #[operation(local)]
        async fn single_paramspec(&self, signatures: &CallableSignature<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_signature(&self, cursor: &mut slice::Iter<'db, Signature<'db>>) -> Result<Option<&'db Signature<'db>>, Self::Error>;
        #[operation(child)]
        async fn with_kind(&self, callable: CallableType<'db>, kind: CallableTypeKind) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn union_elements(&self, union: UnionType<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_union(&self, cursor: &mut slice::Iter<'_, Type<'db>>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn new_union(&self) -> Result<Self::Union, Self::Error>;
        #[operation(child)]
        async fn union_add(&self, builder: &mut Self::Union, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn union_recursion(&self, union: UnionType<'db>) -> Result<RecursivelyDefined, Self::Error>;
        #[operation(child)]
        async fn finish_union(&self, builder: Self::Union, recursion: RecursivelyDefined) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn new_intersection(&self) -> Result<Self::Intersection, Self::Error>;
        #[operation(source)]
        async fn positive_elements(&self, intersection: IntersectionType<'db>) -> Result<Elements<'db>, Self::Error>;
        #[operation(source)]
        async fn negative_elements(&self, intersection: IntersectionType<'db>) -> Result<Elements<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_intersection(&self, cursor: &mut Elements<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn add_positive(&self, builder: &mut Self::Intersection, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn add_negative(&self, builder: &mut Self::Intersection, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn finish_intersection(&self, builder: Self::Intersection) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn transform(&self, ty: Type<'db>, transform: DunderCallableTransform) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl DunderCallableFacts {
        fn classify<'db>(&self, ty: Type<'db>) -> DunderCallableMapping<'db> {
            match ty {
                Type::Callable(callable) => DunderCallableMapping::Callable(callable),
                Type::Union(union) => DunderCallableMapping::Union(union),
                Type::Intersection(intersection) => DunderCallableMapping::Intersection(intersection),
                _ => DunderCallableMapping::Unchanged,
            }
        }
        fn regular(&self, kind: CallableTypeKind) -> bool { kind == CallableTypeKind::Regular }
        fn signatures<'db>(&self, signatures: &'db CallableSignature<'db>) -> slice::Iter<'db, Signature<'db>> { signatures.overloads.iter() }
        fn has_parameters(&self, signature: &Signature<'_>) -> bool { !signature.parameters().as_slice().is_empty() }
        fn elements<'a, 'db>(&self, elements: &'a [Type<'db>]) -> slice::Iter<'a, Type<'db>> { elements.iter() }
        fn rebuild(&self, original: Type<'_>, mapped: Type<'_>) -> bool {
            original != mapped || matches!(mapped, Type::TypeAlias(_))
        }
        fn prefix<'a, 'db>(&self, elements: &'a [Type<'db>], remaining: &slice::Iter<'_, Type<'db>>) -> slice::Iter<'a, Type<'db>> {
            elements[..elements.len() - remaining.len() - 1].iter()
        }
    }

    /// Applies the selected callable binding behavior through unions and positive intersection terms.
    /// Union aliases still trigger ordinary alias-unpacking reconstruction; intersection negatives
    /// retain their original types because this operation only changes callable member binding.
    #[synchronous(dunder_callable_sync)]
    #[capabilities(effects = DunderCallableEffects, facts = DunderCallableFacts)]
    #[passive_values(CallableTypeKind::DunderParamSpec, CallableTypeKind::FunctionLike)]
    pub(in crate::types) async fn dunder_callable_with<'db, E: DunderCallableEffects<'db>>(
        ty: Type<'db>, transform: DunderCallableTransform, facts: DunderCallableFacts, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        effects.checkpoint().await?;
        match facts.classify(ty) {
            DunderCallableMapping::Callable(callable) => {
                if !facts.regular(effects.callable_kind(callable).await?) {
                    return Ok(ty);
                }
                let signatures = effects.signatures(callable).await?;
                match transform {
                    DunderCallableTransform::DunderParamSpec => {
                        if effects.single_paramspec(signatures).await? {
                            return effects.with_kind(callable, CallableTypeKind::DunderParamSpec).await;
                        }
                    }
                    DunderCallableTransform::FunctionLike => {
                        let mut cursor = facts.signatures(signatures);
                        #[cursor_loop]
                        while let Some(signature) = effects.next_signature(&mut cursor).await? {
                            if facts.has_parameters(signature) {
                                return effects.with_kind(callable, CallableTypeKind::FunctionLike).await;
                            }
                        }
                    }
                }
                Ok(ty)
            }
            DunderCallableMapping::Union(union) => {
                let elements = effects.union_elements(union).await?;
                let mut cursor = facts.elements(elements);
                #[cursor_loop]
                while let Some(element) = effects.next_union(&mut cursor).await? {
                    let mapped = effects.transform(element, transform).await?;
                    if facts.rebuild(element, mapped) {
                        let mut builder = effects.new_union().await?;
                        let mut prefix = facts.prefix(elements, &cursor);
                        #[cursor_loop]
                        while let Some(previous) = effects.next_union(&mut prefix).await? {
                            effects.union_add(&mut builder, previous).await?;
                        }
                        effects.union_add(&mut builder, mapped).await?;
                        #[cursor_loop]
                        while let Some(element) = effects.next_union(&mut cursor).await? {
                            let mapped = effects.transform(element, transform).await?;
                            effects.union_add(&mut builder, mapped).await?;
                        }
                        let recursion = effects.union_recursion(union).await?;
                        return effects.finish_union(builder, recursion).await;
                    }
                }
                Ok(ty)
            }
            DunderCallableMapping::Intersection(intersection) => {
                let mut builder = effects.new_intersection().await?;
                let mut positive = effects.positive_elements(intersection).await?;
                #[cursor_loop]
                while let Some(element) = effects.next_intersection(&mut positive).await? {
                    let mapped = effects.transform(element, transform).await?;
                    effects.add_positive(&mut builder, mapped).await?;
                }
                let mut negative = effects.negative_elements(intersection).await?;
                #[cursor_loop]
                while let Some(element) = effects.next_intersection(&mut negative).await? {
                    effects.add_negative(&mut builder, element).await?;
                }
                effects.finish_intersection(builder).await
            }
            DunderCallableMapping::Unchanged => Ok(ty),
        }
    }
}

/// Supplies canonical values and builders to the ordinary dunder-member traversal.
pub(in crate::types) struct OrdinaryDunderCallableEffects<'a, 'db> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) env: &'a ProgramEnvironment<'db>,
}

impl<'db> SynchronousDunderCallableEffects<'db> for OrdinaryDunderCallableEffects<'_, 'db> {
    type Error = Infallible;
    type Union = UnionBuilder<'db>;
    type Intersection = IntersectionBuilder<'db>;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }
    fn callable_kind(&self, callable: CallableType<'db>) -> Result<CallableTypeKind, Infallible> {
        Ok(callable.kind(self.db))
    }
    fn signatures(
        &self,
        callable: CallableType<'db>,
    ) -> Result<&'db CallableSignature<'db>, Infallible> {
        Ok(callable.signatures(self.db))
    }
    fn single_paramspec(&self, signatures: &CallableSignature<'db>) -> Result<bool, Infallible> {
        Ok(signatures.is_single_paramspec().is_some())
    }
    fn next_signature(
        &self,
        cursor: &mut slice::Iter<'db, Signature<'db>>,
    ) -> Result<Option<&'db Signature<'db>>, Infallible> {
        Ok(cursor.next())
    }
    fn with_kind(
        &self,
        callable: CallableType<'db>,
        kind: CallableTypeKind,
    ) -> Result<Type<'db>, Infallible> {
        Ok(Type::Callable(callable.with_kind(self.db, kind)))
    }
    fn union_elements(&self, union: UnionType<'db>) -> Result<&'db [Type<'db>], Infallible> {
        Ok(union.elements(self.db))
    }
    fn next_union(
        &self,
        cursor: &mut slice::Iter<'_, Type<'db>>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(cursor.next().copied())
    }
    fn new_union(&self) -> Result<Self::Union, Infallible> {
        Ok(UnionBuilder::new(self.db, self.env))
    }
    fn union_add(&self, builder: &mut Self::Union, ty: Type<'db>) -> Result<(), Infallible> {
        builder.add_in_place(ty);
        Ok(())
    }
    fn union_recursion(&self, union: UnionType<'db>) -> Result<RecursivelyDefined, Infallible> {
        Ok(union.recursively_defined(self.db))
    }
    fn finish_union(
        &self,
        builder: Self::Union,
        recursion: RecursivelyDefined,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.or_recursively_defined(recursion).build())
    }
    fn new_intersection(&self) -> Result<Self::Intersection, Infallible> {
        Ok(IntersectionBuilder::new(self.db, self.env))
    }
    fn positive_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<Elements<'db>, Infallible> {
        Ok(Elements::Positive(intersection.positive(self.db).iter()))
    }
    fn negative_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<Elements<'db>, Infallible> {
        Ok(Elements::Negative(intersection.negative(self.db).iter()))
    }
    fn next_intersection(
        &self,
        cursor: &mut Elements<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(match cursor {
            Elements::Positive(iter) => iter.next().copied(),
            Elements::Negative(iter) => iter.next().copied(),
        })
    }
    fn add_positive(
        &self,
        builder: &mut Self::Intersection,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        builder.add_positive_in_place(ty);
        Ok(())
    }
    fn add_negative(
        &self,
        builder: &mut Self::Intersection,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        builder.add_negative_in_place(ty);
        Ok(())
    }
    fn finish_intersection(&self, builder: Self::Intersection) -> Result<Type<'db>, Infallible> {
        Ok(builder.build())
    }
    fn transform(
        &self,
        ty: Type<'db>,
        transform: DunderCallableTransform,
    ) -> Result<Type<'db>, Infallible> {
        dunder_callable_sync(ty, transform, DunderCallableFacts, self)
    }
}
