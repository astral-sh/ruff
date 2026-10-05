//! Declaration-owned class bases assembled through canonical expression dependencies.

use std::borrow::Cow;

use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::Definition;

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::analysis::ClassCheckOperation;
use crate::types::class::ExpandedClassBaseEntry;
use crate::types::class::base_entries::{
    BaseEntryDriverEffects, ClassBaseCursor, ClassBaseEntryFacts, ClassBaseEntryWork,
    ClassBaseSource, ClassBaseTypeCursor, ClassTupleCursor, ExplicitBaseEffects, ExplicitBaseFacts,
    base_entry, expanded_class_base_entries_async_with, explicit_base_types_async_with,
    initial_explicit_base_types_with, recover_explicit_base_types_with, type_cursor,
};
use crate::types::tuple::TupleSpec;
use crate::types::{StaticClassLiteral, Type};

fn buffer_quote<T>(prefix: usize, capacity: usize) -> RunResult<(usize, usize)> {
    let work = capacity
        .checked_mul(2)
        .and_then(|n| n.checked_add(prefix))
        .and_then(|n| n.checked_add(4))
        .ok_or(RunError::Contract("class base buffer work overflow"))?;
    let bytes = capacity
        .checked_mul(size_of::<T>())
        .ok_or(RunError::Contract("class base buffer allocation overflow"))?;
    Ok((work, bytes))
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn prepare_class_base_source(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassBaseSource<'db>> {
        let scope = self
            .field(class.field_requests(self.db()).body_scope())
            .await?;
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let prepared = self.access.prepare_existing(file).await?;
        if prepared.file != file {
            return Err(RunError::Contract("prepared class bases file is foreign"));
        }
        let file_scope = self
            .field(scope.read_fields(self.db()).file_scope_id())
            .await?;
        let definition = self
            .local(3, 0, || {
                prepared.index.expect_single_definition(
                    prepared.index.scope(file_scope).node().expect_class(),
                )
            })
            .await?;
        let known = self.field(class.field_requests(self.db()).known()).await?;
        Ok(ClassBaseSource {
            module: prepared.module,
            index: prepared.index,
            scope,
            definition,
            known,
        })
    }

    pub(in crate::types::infer) async fn infer_explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Box<[Type<'db>]>> {
        explicit_base_types_async_with(class, ExplicitBaseFacts, self).await
    }

    pub(in crate::types::infer) async fn initial_explicit_bases(
        &self,
        id: salsa::Id,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Box<[Type<'db>]>> {
        initial_explicit_base_types_with(id, class, self).await
    }

    pub(in crate::types::infer) async fn recover_explicit_bases(
        &self,
        cycle: &salsa::Cycle<'_>,
        previous: &[Type<'db>],
        current: Box<[Type<'db>]>,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Box<[Type<'db>]>> {
        let file = self.static_class_file(class).await?;
        self.check_file_program(file).await?;
        recover_explicit_base_types_with(cycle, previous, current, class, ExplicitBaseFacts, self)
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> BaseEntryDriverEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, _work: ClassBaseEntryWork) -> RunResult<()> {
        self.work(1).await
    }

    async fn empty_entries<'a>(&self) -> RunResult<Vec<ExpandedClassBaseEntry<'a, 'db>>> {
        self.local(2, 0, Vec::new).await
    }

    async fn entries<'a>(
        &self,
        capacity: usize,
    ) -> RunResult<Vec<ExpandedClassBaseEntry<'a, 'db>>> {
        // Each allocation prepays retirement of its initialized entries at any suspension point.
        let (work, bytes) = buffer_quote::<ExpandedClassBaseEntry<'a, 'db>>(0, capacity)?;
        self.local(work, bytes, || Vec::with_capacity(capacity))
            .await
    }

    async fn next_base<'a>(
        &self,
        cursor: &mut ClassBaseCursor<'a>,
    ) -> RunResult<Option<(usize, &'a ast::Expr)>> {
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

    async fn tuple_spec(
        &self,
        _definition: Definition<'db>,
        _ty: Type<'db>,
    ) -> RunResult<Option<Cow<'db, TupleSpec<'db>>>> {
        self.unavailable(SourceOperation::ClassCheck(
            ClassCheckOperation::ExplicitBaseTuple,
        ))
        .await
    }

    async fn next_source<'a>(
        &self,
        cursor: &mut ClassBaseCursor<'a>,
    ) -> RunResult<Option<(usize, &'a ast::Expr)>> {
        self.local(1, 0, || cursor.next()).await
    }

    async fn next_tuple<'a>(
        &self,
        cursor: &mut ClassTupleCursor<'a, 'db>,
    ) -> RunResult<Option<(usize, Type<'db>)>> {
        self.local(1, 0, || cursor.next()).await
    }

    async fn append_entry<'a>(
        &self,
        entries: &mut Vec<ExpandedClassBaseEntry<'a, 'db>>,
        source_node: &'a ast::Expr,
        ty: Type<'db>,
    ) -> RunResult<()> {
        if entries.len() == entries.capacity() {
            let capacity = Self::checked(entries.capacity().checked_mul(2))?.max(1);
            let (work, bytes) =
                buffer_quote::<ExpandedClassBaseEntry<'a, 'db>>(entries.len(), capacity)?;
            self.local(work, bytes, || {
                entries.reserve_exact(capacity - entries.len())
            })
            .await?;
        }
        self.local(3, 0, || entries.push(base_entry(source_node, ty)))
            .await
    }

    async fn publish_entries<'a>(
        &self,
        entries: Vec<ExpandedClassBaseEntry<'a, 'db>>,
    ) -> RunResult<Vec<ExpandedClassBaseEntry<'a, 'db>>> {
        self.work(Self::checked(entries.len().checked_add(1))?)
            .await?;
        Ok(entries)
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ExplicitBaseEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn source(&self, class: StaticClassLiteral<'db>) -> RunResult<ClassBaseSource<'db>> {
        self.prepare_class_base_source(class).await
    }

    async fn expand<'a>(
        &self,
        source: &'a ClassBaseSource<'db>,
    ) -> RunResult<Vec<ExpandedClassBaseEntry<'a, 'db>>> {
        let file_scope = self
            .field(source.scope.read_fields(self.db()).file_scope_id())
            .await?;
        let node = self
            .local(1, 0, || source.node_in_scope(file_scope))
            .await?;
        expanded_class_base_entries_async_with(
            source.known,
            node,
            source.definition,
            ClassBaseEntryFacts,
            self,
        )
        .await
    }

    async fn type_buffer(&self, capacity: usize) -> RunResult<Vec<Type<'db>>> {
        let (work, bytes) = buffer_quote::<Type<'db>>(0, capacity)?;
        self.local(work, bytes, || Vec::with_capacity(capacity))
            .await
    }

    async fn type_cursor<'a>(
        &self,
        entries: Vec<ExpandedClassBaseEntry<'a, 'db>>,
    ) -> RunResult<ClassBaseTypeCursor<'a, 'db>> {
        // The buffer's creation and growth already paid for dropping the remaining iterator.
        self.work(1).await?;
        Ok(type_cursor(entries))
    }

    async fn next_type(
        &self,
        cursor: &mut ClassBaseTypeCursor<'_, 'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(1, 0, || cursor.next()).await
    }

    async fn append_type(&self, types: &mut Vec<Type<'db>>, ty: Type<'db>) -> RunResult<()> {
        if types.len() == types.capacity() {
            let capacity = Self::checked(types.capacity().checked_mul(2))?.max(1);
            let (work, bytes) = buffer_quote::<Type<'db>>(types.len(), capacity)?;
            self.local(work, bytes, || types.reserve_exact(capacity - types.len()))
                .await?;
        }
        self.local(3, 0, || types.push(ty)).await
    }

    async fn box_types(&self, types: Vec<Type<'db>>) -> RunResult<Box<[Type<'db>]>> {
        let (work, allocation) = buffer_quote::<Type<'db>>(types.capacity(), types.len())?;
        let bytes = if types.capacity() == types.len() {
            0
        } else {
            allocation
        };
        // Retain the owner outside the admission closure until the runtime has drained children.
        let mut owner = Some(types);
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(work)?;
                if bytes != 0 {
                    endpoint.admit(salsa::execution_probe::ExecutionWork::Resource {
                        requested_bytes: bytes,
                    })?;
                }
                endpoint.check_completion()?;
                let types = owner
                    .take()
                    .ok_or(RunError::Contract("class base buffer already consumed"))?;
                Ok(types.into_boxed_slice())
            })
            .await)
    }

    async fn publish_types(&self, types: Box<[Type<'db>]>) -> RunResult<Box<[Type<'db>]>> {
        self.work(Self::checked(types.len().checked_add(1))?)
            .await?;
        Ok(types)
    }

    async fn seed(
        &self,
        source: &ClassBaseSource<'db>,
        id: salsa::Id,
    ) -> RunResult<Box<[Type<'db>]>> {
        let file_scope = self
            .field(source.scope.read_fields(self.db()).file_scope_id())
            .await?;
        let len = self
            .local(1, 0, || source.node_in_scope(file_scope).bases().len())
            .await?;
        let (work, bytes) = buffer_quote::<Type<'db>>(0, len)?;
        self.local(work, bytes, || {
            vec![Type::divergent(id); len].into_boxed_slice()
        })
        .await
    }

    async fn recovery_checkpoint(&self) -> RunResult<bool> {
        self.local(1, 0, || true).await
    }

    async fn normalize(
        &self,
        _cycle: &salsa::Cycle<'_>,
        _previous: &[Type<'db>],
        _current: Box<[Type<'db>]>,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<Box<[Type<'db>]>> {
        self.unavailable(SourceOperation::ClassCheck(
            ClassCheckOperation::ExplicitBaseCycleNormalization,
        ))
        .await
    }
}
