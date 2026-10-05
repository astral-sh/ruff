//! Class finality uses the canonical enum metadata before subclass normalization.

use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::ast_node_ref::AstNodeRef;
use ty_python_core::definition::Definition;

use super::{PreparedSource, SourceAccess, SourceEffects, SourceOperation};
use crate::{Program, ProgramEnvironment};
use crate::analysis::ClassCheckOperation;
use crate::types::class::DynamicEnumLiteral;
use crate::types::class::static_literal::decorators::{
    ClassDecoratorEffects, DecoratorExpressionCursor, DecoratorFacts, DecoratorTypeCursor,
    KnownClassDecoratorEffects, class_decorators_with, has_known_class_decorator_with,
};
use crate::types::class::static_literal::{StaticFinalityEffects, static_finality_with};
use crate::types::enums::EnumMetadata;
use crate::types::enums::metadata::{EnumMetadataEffects, enum_metadata_with};
use crate::types::function::{FunctionType, KnownFunction};
use crate::types::subclass_of::SubclassConstructionEffects;
use crate::types::{ClassLiteral, ClassType, KnownClass, Specialization, StaticClassLiteral, Type};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn static_class_identity(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>> {
        match class {
            ClassType::NonGeneric(ClassLiteral::Static(class)) => Ok(Some((class, None))),
            ClassType::NonGeneric(_) => Ok(None),
            ClassType::Generic(alias) => {
                let origin = self.field(alias.field_requests(self.db()).origin()).await?;
                let specialization = self
                    .field(alias.field_requests(self.db()).specialization())
                    .await?;
                Ok(Some((origin, Some(specialization))))
            }
        }
    }

    pub(in crate::types::infer) async fn infer_enum_metadata(
        &self,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<EnumMetadata<'db>>> {
        let ClassLiteral::Static(literal) = class else {
            return self
                .unavailable(SourceOperation::ClassCheck(
                    ClassCheckOperation::EnumMetadata,
                ))
                .await;
        };
        let file = self.static_class_file(literal).await?;
        self.check_file_program(file).await?;
        enum_metadata_with(class, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> EnumMetadataEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn known_class(&self, class: StaticClassLiteral<'db>) -> RunResult<Option<KnownClass>> {
        self.field(class.field_requests(self.db()).known()).await
    }

    async fn is_enum_subclass_with_members(&self, class: KnownClass) -> RunResult<bool> {
        self.local(1, 0, || class.is_enum_subclass_with_members())
            .await
    }

    async fn dynamic_enum_metadata(
        &self,
        _class: DynamicEnumLiteral<'db>,
    ) -> RunResult<Option<EnumMetadata<'db>>> {
        self.unavailable(SourceOperation::ClassCheck(
            ClassCheckOperation::EnumMetadata,
        ))
        .await
    }

    async fn static_program_environment(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ProgramEnvironment<'db>> {
        let file = self.static_class_file(class).await?;
        self.local(3, 0, || ProgramEnvironment::from_file(file))
            .await
    }

    async fn is_enum_class_by_inheritance(
        &self,
        class: StaticClassLiteral<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<bool> {
        self.is_enum_class_by_inheritance_source(class, env).await
    }

    async fn static_enum_member_metadata(
        &self,
        _class: StaticClassLiteral<'db>,
        _env: ProgramEnvironment<'db>,
    ) -> RunResult<Option<EnumMetadata<'db>>> {
        self.unavailable(SourceOperation::ClassCheck(
            ClassCheckOperation::EnumMetadata,
        ))
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SubclassConstructionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        // The shared constructor writes one inline Type after this checkpoint.
        self.local(1, size_of::<Type<'db>>(), || ()).await
    }

    async fn is_final(&self, class: ClassType<'db>) -> RunResult<bool> {
        let class = self.static_class_identity(class).await?;
        let Some((class, _)) = class else {
            return self
                .unavailable(SourceOperation::ClassCheck(
                    ClassCheckOperation::SubclassConstruction,
                ))
                .await;
        };
        static_finality_with(class, self).await
    }

    async fn is_object(&self, class: ClassType<'db>) -> RunResult<bool> {
        let class = match class {
            ClassType::NonGeneric(literal) => literal,
            ClassType::Generic(alias) => self
                .field(alias.field_requests(self.db()).origin())
                .await?
                .into(),
        };
        let ClassLiteral::Static(class) = class else {
            return Ok(false);
        };
        Ok(self.field(class.field_requests(self.db()).known()).await? == Some(KnownClass::Object))
    }

    async fn subclass_of_object(&self) -> RunResult<Type<'db>> {
        // `type[object]` normalizes to the canonical instance of `type`.
        // Keep its lookup and publication in the installed known-class query.
        // Each of the three arguments is obtained, initialized, and bound (nine operations);
        // the call and forwarding its future add two. The two tuples cover evaluated arguments
        // and callee bindings; the boxing helper separately covers captures, storage, and results.
        let bytes = size_of::<[(&A, Program<'db>, KnownClass); 2]>();
        self.boxed_future_with_fixed_transfers(Ok((11, bytes)), || {
            self.access
                .known_class_instance(self.program, KnownClass::Type)
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> StaticFinalityEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn has_decorators(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.field(class.field_requests(self.db()).has_decorators())
            .await
    }

    async fn has_final_decorator(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        has_known_class_decorator_with(class, KnownFunction::Final, DecoratorFacts, self).await
    }

    async fn has_enum_metadata(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        Ok(self.access.enum_metadata(class).await?.is_some())
    }
}

pub(in crate::types::infer) struct SourceClassDecoratorSource<'db> {
    prepared: PreparedSource<'db>,
    node: &'db AstNodeRef<ast::StmtClassDef>,
}

fn decorator_buffer_quote(prefix: usize, capacity: usize) -> RunResult<(usize, usize)> {
    let work = capacity
        .checked_mul(2)
        .and_then(|work| work.checked_add(prefix))
        .and_then(|work| work.checked_add(4))
        .ok_or(RunError::Contract("class decorator buffer work overflow"))?;
    let bytes = capacity
        .checked_mul(size_of::<Type<'_>>())
        .ok_or(RunError::Contract(
            "class decorator buffer allocation overflow",
        ))?;
    Ok((work, bytes))
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn infer_class_decorators(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Box<[Type<'db>]>> {
        class_decorators_with(class, DecoratorFacts, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassDecoratorEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Source = SourceClassDecoratorSource<'db>;
    type Buffer = Vec<Type<'db>>;

    async fn source(&self, class: StaticClassLiteral<'db>) -> RunResult<Self::Source> {
        let scope = self
            .field(
                class
                    .field_requests(self.access.endpoint().field_request_context())
                    .body_scope(),
            )
            .await?;
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let prepared = self.access.prepare_existing(file).await?;
        if prepared.file != file {
            return Err(RunError::Contract(
                "prepared class decorators file is foreign",
            ));
        }
        let file_scope = self
            .field(
                scope
                    .read_fields(self.access.endpoint().field_request_context())
                    .file_scope_id(),
            )
            .await?;
        let node = self
            .local(2, 0, || {
                prepared.index.scope(file_scope).node().expect_class()
            })
            .await?;
        Ok(SourceClassDecoratorSource { prepared, node })
    }
    async fn len(&self, source: &Self::Source) -> RunResult<usize> {
        self.local(2, 0, || {
            source
                .node
                .node(&source.prepared.module)
                .decorator_list
                .len()
        })
        .await
    }
    async fn empty(&self) -> RunResult<Box<[Type<'db>]>> {
        self.local(1, 0, Box::<[Type<'db>]>::default).await
    }
    async fn definition(
        &self,
        _class: StaticClassLiteral<'db>,
        source: &Self::Source,
    ) -> RunResult<Definition<'db>> {
        self.local(3, 0, || {
            source
                .prepared
                .index
                .expect_single_definition(source.node.node(&source.prepared.module))
        })
        .await
    }
    async fn buffer(&self, capacity: usize) -> RunResult<Self::Buffer> {
        let (work, bytes) = decorator_buffer_quote(0, capacity)?;
        self.local(work, bytes, || Vec::with_capacity(capacity))
            .await
    }
    async fn expression_cursor<'source>(
        &self,
        source: &'source Self::Source,
    ) -> RunResult<DecoratorExpressionCursor<'source>> {
        self.local(2, 0, || {
            DecoratorExpressionCursor::new(
                &source.node.node(&source.prepared.module).decorator_list,
            )
        })
        .await
    }
    async fn next_expression<'source>(
        &self,
        cursor: &mut DecoratorExpressionCursor<'source>,
    ) -> RunResult<Option<&'source ast::Expr>> {
        self.local(1, 0, || cursor.next()).await
    }
    async fn expression_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        self.definition_expression_type(definition, expression)
            .await
    }
    async fn append(
        &self,
        class: StaticClassLiteral<'db>,
        buffer: &mut Self::Buffer,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.local(3, 0, || {
            if buffer.len() == buffer.capacity() {
                return Err(RunError::Contract(
                    "class decorator buffer exceeds source length",
                ));
            }
            buffer.push(ty);
            Ok(())
        })
        .await??;
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::class_decorators::observe_decorator_insert(
            self.db(),
            class,
        );
        #[cfg(not(test))]
        let _ = class;
        Ok(())
    }
    async fn finish(&self, buffer: Self::Buffer) -> RunResult<Box<[Type<'db>]>> {
        let (work, allocation) = decorator_buffer_quote(buffer.capacity(), buffer.len())?;
        let bytes = if buffer.capacity() == buffer.len() {
            0
        } else {
            allocation
        };
        let mut owner = Some(buffer);
        self.local(work, bytes, || {
            owner
                .take()
                .map(Vec::into_boxed_slice)
                .ok_or(RunError::Contract(
                    "class decorator buffer already consumed",
                ))
        })
        .await?
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> KnownClassDecoratorEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    async fn has_decorators(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.field(
            class
                .field_requests(self.access.endpoint().field_request_context())
                .has_decorators(),
        )
        .await
    }
    async fn decorators(&self, class: StaticClassLiteral<'db>) -> RunResult<&'db [Type<'db>]> {
        self.access.class_decorators(class).await
    }
    async fn cursor(&self, elements: &'db [Type<'db>]) -> RunResult<DecoratorTypeCursor<'db>> {
        self.local(1, 0, || DecoratorTypeCursor::new(elements))
            .await
    }
    async fn next_type(
        &self,
        cursor: &mut DecoratorTypeCursor<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(1, 0, || cursor.next()).await
    }
    async fn known_function(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<Option<KnownFunction>> {
        let fields = self.access.endpoint().field_request_context();
        let literal = self
            .field(function.field_requests(fields).literal())
            .await?;
        self.field(literal.last_definition.field_requests(fields).known())
            .await
    }
}
