use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::data_descriptor::{
    DataDescriptorEffects, DataDescriptorElements, DataDescriptorFacts,
    classify_data_descriptor_with,
};
use crate::types::{
    IntersectionType, MemberLookupPolicy, RecursiveType, Type, TypeAliasType, UnionType,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn infer_data_descriptor(
        &self,
        ty: Type<'db>,
        any_of_union: bool,
    ) -> RunResult<bool> {
        classify_data_descriptor_with(ty, any_of_union, DataDescriptorFacts, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> DataDescriptorEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(4).await
    }

    async fn union_elements(
        &self,
        union: UnionType<'db>,
    ) -> RunResult<DataDescriptorElements<'db>> {
        let elements = self.union_elements_source(union).await?;
        self.initialize_value(|| DataDescriptorElements::Union(elements))
            .await
    }

    async fn intersection_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<DataDescriptorElements<'db>> {
        let elements = self
            .field(
                intersection
                    .field_requests(self.access.endpoint().field_request_context())
                    .positive(),
            )
            .await?;
        self.initialize_value(|| DataDescriptorElements::Intersection(elements))
            .await
    }

    async fn next_element(
        &self,
        elements: &DataDescriptorElements<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(4, 0, || elements.next(cursor)).await
    }

    async fn child(&self, ty: Type<'db>, any_of_union: bool) -> RunResult<bool> {
        self.access.data_descriptor(self.program, ty, any_of_union).await
    }

    async fn alias_value(&self, _alias: TypeAliasType<'db>) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::TypeAliasResolution).await
    }

    async fn unfold(&self, _recursive: RecursiveType<'db>) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::RecursiveTypeUnfold).await
    }

    async fn has_member(&self, ty: Type<'db>, name: &'static str) -> RunResult<bool> {
        let name = self.initialize_value(|| Name::new_static(name)).await?;
        let member = self
            .access
            .class_member_lookup(ty, &name, MemberLookupPolicy::REQUIRE_CONCRETE)
            .await?;
        self.local(1, 0, || !member.place.is_undefined()).await
    }
}
