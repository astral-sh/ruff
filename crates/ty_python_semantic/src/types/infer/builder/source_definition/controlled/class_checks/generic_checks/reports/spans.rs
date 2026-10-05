//! Canonical report dependencies followed by finite source-range selection.

mod cost;

use ruff_db::diagnostic::Span;
use ruff_db::files::File;
use ruff_db::parsed::ParsedModuleRef;
use ruff_python_ast::name::Name;
use ruff_text_size::TextRange;
use salsa::execution_probe::{FieldReadProfile, FieldRequest, FieldRequestContext, RunResult};
use ty_python_core::definition::Definition;
use ty_python_core::scope::{NodeWithScopeKind, ScopeId};
use ty_python_core::ProgramFile;

use super::{ClassReports, checked};
use crate::types::class::{
    DynamicClassAnchor, DynamicClassScopeOffset, DynamicEnumAnchor, DynamicNamedTupleAnchor,
    DynamicTypedDictAnchor, dynamic_class_definition_header_range, dynamic_class_offset_header_range,
};
use crate::types::function::{FunctionType, OverloadLiteral};
use crate::types::infer::DefinitionTypes;
use crate::types::infer::builder::source_definition::controlled::SourceAccess;
use crate::types::infer::builder::source_definition::controlled::class_selection::{FixedFieldBorrow, FixedFieldCopy};
use crate::types::local_transfer::{boxed_future_with_fixed_transfers_at, generated_field_quote};
use crate::types::typevar::{BoundTypeVarIdentity, TypeVarIdentity, TypeVarInstance};
use crate::types::{BoundTypeVarInstance, ClassLiteral, StaticClassLiteral, Type, TypeVarKind};

/// The finite source anchor retained after reading a dynamic class's interned field.
#[derive(Debug, Clone, Copy)]
enum DynamicSpanAnchor<'db> {
    Definition(Definition<'db>),
    ScopeOffset(ScopeId<'db>, DynamicClassScopeOffset),
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassReports<'_, '_, '_, 'run, 'db, '_, A> {
    /// Reads one generated copied or borrowed field after admitting its request and fixed transfers.
    async fn field<H: Copy, F, R, P, Accessor>(
        &self, handle: H, accessor: impl FnOnce(H, FieldRequestContext<'db>) -> Accessor,
        request: F, profile: &P,
    ) -> RunResult<R::Output>
    where
        F: Copy + FnOnce(H, FieldRequestContext<'db>) -> R,
        R: FieldRequest<'db>,
        P: FieldReadProfile<R::Stored>,
    {
        let endpoint = self.checks.source.access.endpoint();
        let quote = generated_field_quote(accessor, request)?;
        let quote = self.local(8, 8 * size_of::<Option<usize>>(), || -> RunResult<(usize, usize)> {
            Ok((checked(quote.0.checked_add(16))?, checked(quote.1.checked_add(size_of::<[(H, F, &P, &Self); 2]>()))?))
        }).await??;
        let read = boxed_future_with_fixed_transfers_at(endpoint, Ok(quote), || {
            endpoint.read_field(request(handle, endpoint.field_request_context()), profile)
        }).await?;
        Ok(read.await)
    }

    async fn identity(&self, variable: TypeVarInstance<'db>) -> RunResult<TypeVarIdentity<'db>> {
        self.field(variable, |v, c| v.field_requests(c), |v, c| v.field_requests(c).identity(), &FixedFieldCopy).await
    }

    async fn bound_identity(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<BoundTypeVarIdentity<'db>> {
        self.field(variable, |v, c| v.field_requests(c), |v, c| v.identity_request(c), &FixedFieldCopy).await
    }

    pub(super) async fn variable_name(&self, variable: TypeVarInstance<'db>) -> RunResult<&'db Name> {
        let identity = self.identity(variable).await?;
        self.field(identity, |v, c| v.field_requests(c), |v, c| v.field_requests(c).name(), &FixedFieldBorrow).await
    }

    pub(super) async fn bound_name(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<&'db Name> {
        let variable = self.field(variable, |v, c| v.field_requests(c), |v, c| v.field_requests(c).typevar(), &FixedFieldCopy).await?;
        self.variable_name(variable).await
    }

    pub(super) async fn variable_bound_kind(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<TypeVarKind> {
        let identity = self.bound_identity(variable).await?;
        self.field(identity.identity, |v, c| v.field_requests(c), |v, c| v.field_requests(c).kind(), &FixedFieldCopy).await
    }

    pub(super) async fn variable_binding_definition(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<Option<Definition<'db>>> {
        let identity = self.bound_identity(variable).await?;
        self.local(14, size_of::<[BoundTypeVarIdentity<'db>; 2]>(), || identity.binding_context.definition()).await
    }

    async fn definition_scope(&self, definition: Definition<'db>) -> RunResult<ScopeId<'db>> {
        self.field(definition, |d, c| d.read_fields(c), |d, c| d.read_fields(c).scope_id(), &FixedFieldCopy).await
    }

    async fn scope_file(&self, scope: ScopeId<'db>) -> RunResult<ProgramFile<'db>> {
        self.field(scope, |s, c| s.read_fields(c), |s, c| s.read_fields(c).program_file(), &FixedFieldCopy).await
    }

    async fn physical_file(&self, file: ProgramFile<'db>) -> RunResult<File> {
        let python_file = self.field(file, |f, c| f.read_fields(c), |f, c| f.read_fields(c).python_file(), &FixedFieldCopy).await?;
        self.field(python_file, |f, c| f.read_fields(c), |f, c| f.read_fields(c).file(), &FixedFieldCopy).await
    }

    /// Returns the canonical parsed module for a file. Callers retain this owner while
    /// inspecting indexed nodes and awaiting subsequent span dependencies.
    async fn parsed(&self, file: ProgramFile<'db>) -> RunResult<ParsedModuleRef> {
        let bytes = size_of::<[(&A, ProgramFile<'db>); 2]>();
        boxed_future_with_fixed_transfers_at(self.checks.source.access.endpoint(), Ok((13, bytes)), || {
            self.checks.source.access.parsed_module(file)
        }).await?.await
    }

    async fn scope_node(&self, scope: ScopeId<'db>, file: ProgramFile<'db>) -> RunResult<&'db NodeWithScopeKind> {
        let id = self.field(scope, |s, c| s.read_fields(c), |s, c| s.read_fields(c).file_scope_id(), &FixedFieldCopy).await?;
        let bytes = size_of::<[(&A, ProgramFile<'db>); 2]>();
        let index = boxed_future_with_fixed_transfers_at(self.checks.source.access.endpoint(), Ok((13, bytes)), || {
            self.checks.source.access.semantic_index(file)
        }).await?.await?;
        self.local(cost::SCOPE_NODE_WORK, cost::SCOPE_NODE_BYTES, || index.scope(id).node()).await
    }

    pub(super) async fn static_class_range(&self, class: StaticClassLiteral<'db>) -> RunResult<TextRange> {
        let scope = self.field(class, |c, f| c.field_requests(f), |c, f| c.field_requests(f).body_scope(), &FixedFieldCopy).await?;
        let file = self.scope_file(scope).await?;
        let module = self.parsed(file).await?;
        let node = self.scope_node(scope, file).await?;
        self.local(cost::CLASS_RANGE_WORK, cost::CLASS_RANGE_BYTES, || {
            StaticClassLiteral::header_range_from_node(node.expect_class().node(&module))
        }).await
    }

    pub(super) async fn variable_definition_span(&self, variable: TypeVarInstance<'db>) -> RunResult<Option<Span>> {
        let identity = self.identity(variable).await?;
        let definition = self.field(identity, |v, c| v.field_requests(c), |v, c| v.field_requests(c).definition(), &FixedFieldCopy).await?;
        let Some(definition) = definition else { return Ok(None); };
        let scope = self.definition_scope(definition).await?;
        let file = self.scope_file(scope).await?;
        let physical = self.physical_file(file).await?;
        let module = self.parsed(file).await?;
        let kind = self.field(definition, |d, c| d.read_fields(c), |d, c| d.read_fields(c).kind(), &FixedFieldBorrow).await?;
        self.local(cost::DEFINITION_RANGE_WORK, cost::DEFINITION_RANGE_BYTES, || {
            Some(Span::from(physical).with_range(kind.full_range(&module)))
        }).await
    }

    /// Reads the real definition query and uses its ordinary binding and cycle-recovery selection.
    pub(super) async fn canonical_binding_type(&self, definition: Definition<'db>) -> RunResult<Type<'db>> {
        let bytes = size_of::<[(&A, Definition<'db>); 2]>();
        let inference = boxed_future_with_fixed_transfers_at(self.checks.source.access.endpoint(), Ok((13, bytes)), || {
            self.checks.source.access.definition(definition)
        }).await?.await?;
        let quote = self.local(cost::BINDING_PREPARATION_WORK, cost::BINDING_PREPARATION_BYTES, || {
            let entries = match &inference.types {
                DefinitionTypes::Binding(_) | DefinitionTypes::BindingAndDeclaration(_) => 1,
                DefinitionTypes::Other(other) => other.bindings.len(),
                DefinitionTypes::Empty | DefinitionTypes::Declaration(_) => 0,
            };
            cost::binding_quote(entries)
        }).await??;
        self.local(quote.0, quote.1, || inference.binding_type(definition)).await
    }

    pub(super) async fn function_signature_span(&self, function: FunctionType<'db>) -> RunResult<Span> {
        let literal = self.field(function, |f, c| f.field_requests(c), |f, c| f.field_requests(c).literal(), &FixedFieldCopy).await?;
        let scope = self.field(literal.last_definition, |f, c| f.field_requests(c), |f, c| f.field_requests(c).body_scope(), &FixedFieldCopy).await?;
        let file = self.scope_file(scope).await?;
        let physical = self.physical_file(file).await?;
        let module = self.parsed(file).await?;
        let node = self.scope_node(scope, file).await?;
        self.local(cost::FUNCTION_RANGE_WORK, cost::FUNCTION_RANGE_BYTES, || {
            Span::from(physical).with_range(OverloadLiteral::signature_range_from_node(node.expect_function().node(&module)))
        }).await
    }

    pub(super) async fn class_header_span(&self, class: ClassLiteral<'db>) -> RunResult<Span> {
        let anchor = match class {
            ClassLiteral::Static(class) => {
                let scope = self.field(class, |c, f| c.field_requests(f), |c, f| c.field_requests(f).body_scope(), &FixedFieldCopy).await?;
                let file = self.scope_file(scope).await?;
                let physical = self.physical_file(file).await?;
                let range = self.static_class_range(class).await?;
                return self.local(cost::SPAN_WORK, cost::SPAN_BYTES, || Span::from(physical).with_range(range)).await;
            }
            ClassLiteral::Dynamic(class) => {
                let anchor = self.field(class, |c, f| c.field_requests(f), |c, f| c.field_requests(f).anchor(), &FixedFieldBorrow).await?;
                self.local(12, size_of::<[DynamicSpanAnchor<'db>; 2]>(), || match anchor {
                    DynamicClassAnchor::Definition(definition) => DynamicSpanAnchor::Definition(*definition),
                    DynamicClassAnchor::ScopeOffset { scope, offset, .. } => DynamicSpanAnchor::ScopeOffset(*scope, *offset),
                }).await?
            }
            ClassLiteral::DynamicNamedTuple(class) => {
                let anchor = self.field(class, |c, f| c.field_requests(f), |c, f| c.field_requests(f).anchor(), &FixedFieldBorrow).await?;
                self.local(12, size_of::<[DynamicSpanAnchor<'db>; 2]>(), || match anchor {
                    DynamicNamedTupleAnchor::CollectionsDefinition { definition, .. } | DynamicNamedTupleAnchor::TypingDefinition(definition) => DynamicSpanAnchor::Definition(*definition),
                    DynamicNamedTupleAnchor::ScopeOffset { scope, offset, .. } => DynamicSpanAnchor::ScopeOffset(*scope, *offset),
                }).await?
            }
            ClassLiteral::DynamicTypedDict(class) => {
                let anchor = self.field(class, |c, f| c.field_requests(f), |c, f| c.field_requests(f).anchor(), &FixedFieldBorrow).await?;
                self.local(12, size_of::<[DynamicSpanAnchor<'db>; 2]>(), || match anchor {
                    DynamicTypedDictAnchor::Definition(definition) => DynamicSpanAnchor::Definition(*definition),
                    DynamicTypedDictAnchor::ScopeOffset { scope, offset, .. } => DynamicSpanAnchor::ScopeOffset(*scope, *offset),
                }).await?
            }
            ClassLiteral::DynamicEnum(class) => {
                let anchor = self.field(class, |c, f| c.field_requests(f), |c, f| c.field_requests(f).anchor(), &FixedFieldBorrow).await?;
                self.local(12, size_of::<[DynamicSpanAnchor<'db>; 2]>(), || match anchor {
                    DynamicEnumAnchor::Definition { definition, .. } => DynamicSpanAnchor::Definition(*definition),
                    DynamicEnumAnchor::ScopeOffset { scope, offset, .. } => DynamicSpanAnchor::ScopeOffset(*scope, *offset),
                }).await?
            }
        };
        match anchor {
            DynamicSpanAnchor::Definition(definition) => {
                let scope = self.definition_scope(definition).await?;
                let file = self.scope_file(scope).await?;
                let physical = self.physical_file(file).await?;
                let module = self.parsed(file).await?;
                let kind = self.field(definition, |d, c| d.read_fields(c), |d, c| d.read_fields(c).kind(), &FixedFieldBorrow).await?;
                self.local(cost::DYNAMIC_RANGE_WORK, cost::DYNAMIC_RANGE_BYTES, || {
                    Span::from(physical).with_range(dynamic_class_definition_header_range(kind, &module))
                }).await
            }
            DynamicSpanAnchor::ScopeOffset(scope, offset) => {
                let file = self.scope_file(scope).await?;
                let physical = self.physical_file(file).await?;
                let module = self.parsed(file).await?;
                let node = self.scope_node(scope, file).await?;
                self.local(cost::DYNAMIC_RANGE_WORK, cost::DYNAMIC_RANGE_BYTES, || {
                    Span::from(physical).with_range(dynamic_class_offset_header_range(node, offset, &module))
                }).await
            }
        }
    }
}
