use std::convert::Infallible;

use crate::types::{
    IntersectionType, MemberLookupPolicy, RecursiveType, Type, TypeAliasType, UnionType,
};
use crate::{Db, FxOrderSet, Program, ProgramEnvironment};

pub(in crate::types) struct DataDescriptorFacts;

pub(in crate::types) enum DataDescriptorStep<'db> {
    Complete(bool),
    Union(UnionType<'db>),
    Intersection(IntersectionType<'db>),
    Alias(TypeAliasType<'db>),
    Recursive(RecursiveType<'db>),
    Members,
}

pub(in crate::types) enum DataDescriptorElements<'db> {
    Union(&'db [Type<'db>]),
    Intersection(&'db FxOrderSet<Type<'db>>),
}

impl<'db> DataDescriptorElements<'db> {
    pub(in crate::types) fn next(&self, cursor: &mut usize) -> Option<Type<'db>> {
        let ty = match self {
            Self::Union(elements) => elements.get(*cursor),
            Self::Intersection(elements) => elements.get_index(*cursor),
        };
        if ty.is_some() {
            *cursor += 1;
        }
        ty.copied()
    }
}

pub(super) struct OrdinaryDataDescriptorEffects<'db> {
    pub(super) db: &'db dyn Db,
    pub(super) program: Program<'db>,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousDataDescriptorEffects)]
    pub(in crate::types) trait DataDescriptorEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn union_elements(&self, union: UnionType<'db>) -> Result<DataDescriptorElements<'db>, Self::Error>;
        #[operation(source)]
        async fn intersection_elements(&self, intersection: IntersectionType<'db>) -> Result<DataDescriptorElements<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_element(&self, elements: &DataDescriptorElements<'db>, cursor: &mut usize) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn child(&self, ty: Type<'db>, any_of_union: bool) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn alias_value(&self, alias: TypeAliasType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn unfold(&self, recursive: RecursiveType<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn has_member(&self, ty: Type<'db>, name: &'static str) -> Result<bool, Self::Error>;
    }

    #[finite_capability]
    impl DataDescriptorFacts {
        fn union_short_circuits(&self, result: bool, any_of_union: bool) -> bool {
            result == any_of_union
        }

        fn classify<'db>(&self, ty: Type<'db>, any_of_union: bool) -> DataDescriptorStep<'db> {
            match ty {
                Type::Dynamic(_) => DataDescriptorStep::Complete(!any_of_union),
                Type::SubclassOf(_) if ty.dynamic_descriptor_type().is_some() => DataDescriptorStep::Complete(true),
                Type::Never | Type::PropertyInstance(_) | Type::SlotDescriptor(_) => DataDescriptorStep::Complete(true),
                Type::Union(union) => DataDescriptorStep::Union(union),
                Type::Intersection(intersection) => DataDescriptorStep::Intersection(intersection),
                Type::TypeAlias(alias) => DataDescriptorStep::Alias(alias),
                Type::Recursive(recursive) => DataDescriptorStep::Recursive(recursive),
                _ => DataDescriptorStep::Members,
            }
        }
    }

    #[synchronous(classify_data_descriptor_sync)]
    #[capabilities(effects = DataDescriptorEffects, facts = DataDescriptorFacts)]
    #[passive_values()]
    pub(in crate::types) async fn classify_data_descriptor_with<'db, E: DataDescriptorEffects<'db>>(
        ty: Type<'db>,
        any_of_union: bool,
        facts: DataDescriptorFacts,
        effects: &E,
    ) -> Result<bool, E::Error> {
        effects.checkpoint().await?;
        match facts.classify(ty, any_of_union) {
            DataDescriptorStep::Complete(result) => Ok(result),
            DataDescriptorStep::Union(union) => {
                let elements = effects.union_elements(union).await?;
                let mut cursor = 0;
                #[cursor_loop]
                while let Some(element) = effects.next_element(&elements, &mut cursor).await? {
                    let result = effects.child(element, any_of_union).await?;
                    if facts.union_short_circuits(result, any_of_union) {
                        return Ok(any_of_union);
                    }
                }
                Ok(!any_of_union)
            }
            DataDescriptorStep::Intersection(intersection) => {
                let elements = effects.intersection_elements(intersection).await?;
                let mut cursor = 0;
                #[cursor_loop]
                while let Some(element) = effects.next_element(&elements, &mut cursor).await? {
                    if effects.child(element, any_of_union).await? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            DataDescriptorStep::Alias(alias) => {
                let value = effects.alias_value(alias).await?;
                effects.child(value, any_of_union).await
            }
            DataDescriptorStep::Recursive(recursive) => {
                if let Some(unfolded) = effects.unfold(recursive).await? {
                    effects.child(unfolded, any_of_union).await
                } else {
                    Ok(!any_of_union)
                }
            }
            DataDescriptorStep::Members => {
                if effects.has_member(ty, "__set__").await? {
                    Ok(true)
                } else {
                    effects.has_member(ty, "__delete__").await
                }
            }
        }
    }
}

impl<'db> SynchronousDataDescriptorEffects<'db> for OrdinaryDataDescriptorEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn union_elements(
        &self,
        union: UnionType<'db>,
    ) -> Result<DataDescriptorElements<'db>, Self::Error> {
        Ok(DataDescriptorElements::Union(union.elements(self.db)))
    }

    fn intersection_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<DataDescriptorElements<'db>, Self::Error> {
        Ok(DataDescriptorElements::Intersection(intersection.positive(self.db)))
    }

    fn next_element(
        &self,
        elements: &DataDescriptorElements<'db>,
        cursor: &mut usize,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(elements.next(cursor))
    }

    fn child(&self, ty: Type<'db>, any_of_union: bool) -> Result<bool, Self::Error> {
        Ok(ty.is_data_descriptor_impl(self.db, self.program, any_of_union))
    }

    fn alias_value(&self, alias: TypeAliasType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(alias.value_type(self.db))
    }

    fn unfold(&self, recursive: RecursiveType<'db>) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(recursive
            .unfold(self.db, &ProgramEnvironment::from_program(self.program))
            .into_unfolded())
    }

    fn has_member(&self, ty: Type<'db>, name: &'static str) -> Result<bool, Self::Error> {
        Ok(!ty
            .class_member_with_policy(
                self.db,
                &ProgramEnvironment::from_program(self.program),
                name,
                MemberLookupPolicy::REQUIRE_CONCRETE,
            )
            .place
            .is_undefined())
    }
}
