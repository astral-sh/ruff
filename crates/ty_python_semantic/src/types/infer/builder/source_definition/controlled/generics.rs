//! Admitted lexical binding and ordered generic-context construction.

use std::hash::BuildHasherDefault;
use std::iter::{Copied, Once};
use std::slice;

use itertools::Either;
use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::{Definition, DefinitionKind, DefinitionNodeKey};
use ty_python_core::scope::{FileScopeId, Scope};
use ty_python_core::{AncestorsIter, SemanticIndex};

use super::storage::{dense_finish, ordered_merge, sequence_merge, slots};
use super::class_selection::FixedFieldBorrow;
use super::{FixedFieldCopy, SourceAccess, SourceEffects, SourceOperation};
use crate::types::generics::binding::{
    BindingFacts, BindingNode, TypeVarBindingEffects, binding_visible_with, find_in_context_with,
    find_typevar_binding_with, scope_context_with,
};
use crate::types::generics::context_construction::{
    ContextConstructionEffects, ContextVariables, context_from_typevars_with,
};
use crate::types::generics::header_effects::{TypeParameterDeclaration, TypeParameterEffects, sealed};
use crate::types::infer::{
    DefinitionDeclaration, DefinitionInferenceExtra, DefinitionTypes, InferredDeclaration,
};
use crate::types::local_transfer::generated_field_quote;
use crate::types::local_transfer::collections::{CALL_1, CALL_2, CALL_3, checked as checked_quote, event_quote};
use crate::types::local_transfer::context_variables::context_variable_at_quote;
use crate::types::local_transfer::scopes::{ancestors_quote, next_ancestor_quote};
use crate::types::signatures::ReturnCallableTypeVarScope;
use crate::types::storage_quote::StorageQuote;
use crate::types::typevar::{BoundTypeVarIdentity, TypeVarIdentity, TypeVarInstance, TypeVarNonce};
use crate::types::{
    BindingContext, BoundTypeVarInstance, ClassLiteral, GenericContext, KnownInstanceType, Type, TypeAndQualifiers,
    TypeVarKind,
};
use crate::{Db, FxOrderMap, FxOrderSet, Program, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn bind_typevar_in_context(
        &self,
        typevar: TypeVarInstance<'db>,
        binding: BindingContext<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        let identity = TypeVarBindingEffects::typevar_identity(self, typevar).await?;
        let identity = self
            .initialize_value(|| {
                BoundTypeVarIdentity::new(identity, binding, None, TypeVarNonce::NONE)
            })
            .await?;
        self.local(
            1,
            size_of::<(TypeVarInstance<'db>, BoundTypeVarIdentity<'db>)>(),
            || (),
        )
        .await?;
        self.access.intern_bound_typevar(typevar, identity).await
    }

    /// Builds a canonical context from collected legacy variables in encounter order.
    /// Admits consumption of the set and construction of the ordered context map.
    pub(in crate::types::infer::builder) async fn context_from_legacy_variables(
        &self,
        env: &ProgramEnvironment<'db>,
        variables: FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> RunResult<GenericContext<'db>> {
        let quote = self.local_with_fixed_transfers(
            20,
            12 * size_of::<usize>() + 12 * size_of::<Option<usize>>(),
            || {
                let work = slots(variables.capacity())
                    .and_then(|slots| slots.checked_add(variables.len()))
                    .and_then(|work| work.checked_add(4))
                    .ok_or(RunError::Contract("generic context input quotation overflow"))?;
                Ok((work, size_of::<ordermap::map::IntoIter<BoundTypeVarInstance<'db>, ()>>()))
            },
        ).await?;
        // The fixed-transfer helper retains the set in its factory until failed admission drains.
        let variables = self.local_quoted_with_fixed_transfers(quote, || variables.into_iter()).await?;
        self.type_parameter_future(|| context_from_typevars_with(self.db(), env, variables, self))
            .await?.await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeVarBindingEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn kind(&self, typevar: TypeVarInstance<'db>) -> RunResult<TypeVarKind> {
        let identity = TypeVarBindingEffects::typevar_identity(self, typevar).await?;
        let quote = generated_field_quote(
            |identity: TypeVarIdentity<'db>, context| identity.field_requests(context),
            |identity: TypeVarIdentity<'db>, context| identity.field_requests(context).kind(),
        );
        let endpoint = self.access.endpoint();
        let read = self.boxed_future_with_fixed_transfers(quote, || {
            let request = identity.field_requests(endpoint.field_request_context()).kind();
            endpoint.read_field(request, &FixedFieldCopy)
        }).await?;
        Ok(read.await)
    }

    async fn typevar_definition(
        &self,
        typevar: TypeVarInstance<'db>,
    ) -> RunResult<Option<Definition<'db>>> {
        let identity = TypeVarBindingEffects::typevar_identity(self, typevar).await?;
        self.field(
            identity
                .field_requests(self.access.endpoint().field_request_context())
                .definition(),
        )
        .await
    }

    async fn definition_scope(&self, definition: Definition<'db>) -> RunResult<FileScopeId> {
        let scope = SourceEffects::definition_scope(self, definition).await?;
        self.field(
            scope
                .read_fields(self.access.endpoint().field_request_context())
                .file_scope_id(),
        )
        .await
    }

    async fn definition_is_class(&self, definition: Definition<'db>) -> RunResult<bool> {
        let kind = self
            .field(
                definition
                    .read_fields(self.access.endpoint().field_request_context())
                    .kind(),
            )
            .await?;
        self.local(1, 0, || matches!(kind, DefinitionKind::Class(_)))
            .await
    }

    async fn bound_identity(
        &self,
        bound: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarIdentity<'db>> {
        let quote = generated_field_quote(
            |bound: BoundTypeVarInstance<'db>, context| bound.field_requests(context),
            |bound: BoundTypeVarInstance<'db>, context| bound.identity_request(context),
        );
        let endpoint = self.access.endpoint();
        let read = self.boxed_future_with_fixed_transfers(quote, || {
            let request = bound.identity_request(endpoint.field_request_context());
            endpoint.read_field(request, &FixedFieldCopy)
        }).await?;
        Ok(read.await)
    }

    async fn typevar_identity(
        &self,
        typevar: TypeVarInstance<'db>,
    ) -> RunResult<TypeVarIdentity<'db>> {
        let quote = generated_field_quote(
            |typevar: TypeVarInstance<'db>, context| typevar.field_requests(context),
            |typevar: TypeVarInstance<'db>, context| typevar.field_requests(context).identity(),
        );
        let endpoint = self.access.endpoint();
        let read = self.boxed_future_with_fixed_transfers(quote, || {
            let request = typevar.field_requests(endpoint.field_request_context()).identity();
            endpoint.read_field(request, &FixedFieldCopy)
        }).await?;
        Ok(read.await)
    }

    async fn bound_typevar(
        &self,
        bound: BoundTypeVarInstance<'db>,
    ) -> RunResult<TypeVarInstance<'db>> {
        let quote = generated_field_quote(
            |bound: BoundTypeVarInstance<'db>, context| bound.field_requests(context),
            |bound: BoundTypeVarInstance<'db>, context| bound.field_requests(context).typevar(),
        );
        let endpoint = self.access.endpoint();
        let read = self.boxed_future_with_fixed_transfers(quote, || {
            let request = bound.field_requests(endpoint.field_request_context()).typevar();
            endpoint.read_field(request, &FixedFieldCopy)
        }).await?;
        Ok(read.await)
    }

    async fn bind(
        &self,
        typevar: TypeVarInstance<'db>,
        definition: Definition<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        let binding = self
            .initialize_value(|| BindingContext::Definition(definition))
            .await?;
        self.bind_typevar_in_context(typevar, binding).await
    }

    async fn ancestors<'index>(
        &self,
        index: &'index SemanticIndex<'db>,
        scope: FileScopeId,
    ) -> RunResult<AncestorsIter<'index>> {
        let (work, bytes) = const { ancestors_quote() }?;
        self.local_with_fixed_transfers(work, bytes, || {
            index.ancestor_scopes(scope)
        })
        .await
    }

    async fn next_ancestor<'index>(
        &self,
        ancestors: &mut AncestorsIter<'index>,
    ) -> RunResult<Option<(FileScopeId, &'index Scope)>> {
        let (work, bytes) = const { next_ancestor_quote() }?;
        self.local_with_fixed_transfers(work, bytes, || ancestors.next())
            .await
    }

    async fn scope<'index>(
        &self,
        index: &'index SemanticIndex<'db>,
        scope: FileScopeId,
    ) -> RunResult<&'index Scope> {
        self.local(1, size_of::<&'index Scope>(), || index.scope(scope))
            .await
    }

    async fn definition(
        &self,
        index: &SemanticIndex<'db>,
        key: DefinitionNodeKey,
    ) -> RunResult<Definition<'db>> {
        let work = Self::checked(index.definition_lookup_work().checked_add(4))?;
        self.local_with_fixed_transfers(work, 0, || index.expect_single_definition(key))
            .await
    }

    async fn class_context(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        let Some(Type::ClassLiteral(ClassLiteral::Static(class))) =
            self.scope_original_class_type(definition).await?
        else {
            return Ok(None);
        };
        self.access.class_generic_context(class).await
    }

    async fn function_context(
        &self,
        definition: Definition<'db>,
        mode: ReturnCallableTypeVarScope,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.local_with_fixed_transfers(4, size_of::<ReturnCallableTypeVarScope>(), || ())
            .await?;
        match mode {
            ReturnCallableTypeVarScope::Lexical => {
                self.unavailable(SourceOperation::TypeVarBindingFunctionContext)
                    .await
            }
            ReturnCallableTypeVarScope::Public => {
                let file = self.definition_file(definition).await?;
                self.check_file_program(file).await?;
                let inference = self
                    .shadow_future(|| self.access.definition(definition))
                    .await?
                    .await?;
                let entries = self
                    .local_with_fixed_transfers(4, 0, || match &inference.types {
                        DefinitionTypes::Empty | DefinitionTypes::Binding(_) => 0,
                        DefinitionTypes::Declaration(_)
                        | DefinitionTypes::BindingAndDeclaration(_) => 1,
                        DefinitionTypes::Other(types) => types.declarations.len(),
                    })
                    .await?;
                let work = Self::checked(
                    entries
                        .checked_mul(12)
                        .and_then(|work| work.checked_add(32)),
                )?;
                // These are the iterator and temporary representations used by function_type's
                // undecorated preference, declaration fallback, and rejected-declaration recovery.
                let fixed_bytes = size_of::<
                    Either<
                        Once<DefinitionDeclaration<'db>>,
                        Copied<slice::Iter<'_, DefinitionDeclaration<'db>>>,
                    >,
                >() + size_of::<Once<DefinitionDeclaration<'db>>>()
                    + 2 * size_of::<slice::Iter<'_, DefinitionDeclaration<'db>>>()
                    + 2 * size_of::<Option<DefinitionDeclaration<'db>>>()
                    + 2 * size_of::<Option<&DefinitionInferenceExtra<'db>>>()
                    + 4 * size_of::<Option<Type<'db>>>()
                    + 4 * size_of::<Type<'db>>()
                    + 4 * size_of::<Option<TypeAndQualifiers<'db>>>()
                    + 4 * size_of::<InferredDeclaration<'db>>()
                    + size_of::<bool>();
                let entry_bytes = 4 * size_of::<DefinitionDeclaration<'db>>()
                    + 4 * size_of::<Option<TypeAndQualifiers<'db>>>()
                    + 4 * size_of::<InferredDeclaration<'db>>()
                    + 2 * size_of::<bool>();
                let bytes = Self::checked(
                    entries
                        .checked_mul(entry_bytes)
                        .and_then(|bytes| bytes.checked_add(fixed_bytes)),
                )?;
                let Some(function) = self
                    .local_with_fixed_transfers(work, bytes, || inference.function_type(definition))
                    .await?
                else {
                    return Ok(None);
                };
                let signature = self.function_last_definition_signature(function).await?;
                self.local_with_fixed_transfers(2, 0, || signature.generic_context)
                    .await
            }
        }
    }

    async fn alias_context(
        &self,
        _definition: Definition<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.unavailable(SourceOperation::TypeVarBindingAliasContext)
            .await
    }

    async fn captured_paramspec(
        &self,
        _definition: Definition<'db>,
        _typevar: TypeVarInstance<'db>,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        self.unavailable(SourceOperation::TypeVarBindingCapturedParamSpec)
            .await
    }

    async fn variables(
        &self,
        context: GenericContext<'db>,
    ) -> RunResult<&'db ContextVariables<'db>> {
        let quote = generated_field_quote(
            |context: GenericContext<'db>, fields| context.field_requests(fields),
            |context: GenericContext<'db>, fields| context.variables_request(fields),
        );
        let endpoint = self.access.endpoint();
        let read = self.boxed_future_with_fixed_transfers(quote, || {
            let request = context.variables_request(endpoint.field_request_context());
            endpoint.read_field(request, &FixedFieldBorrow)
        }).await?;
        Ok(read.await)
    }

    async fn next_variable(
        &self,
        variables: &ContextVariables<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        let (work, lookup_bytes) = const { context_variable_at_quote() }?;
        // Each step also funds find_in_context_with's comparison of Salsa identity handles
        // and its branch, result construction and loop back edge. Field reads stay separate.
        let (scan_work, scan_bytes) = const {
            checked_quote(event_quote(
                CALL_3 + 6 * CALL_2 + 2 * CALL_1 + 16 + 24,
                &[
                    size_of::<(&BindingFacts, TypeVarIdentity<'static>, TypeVarIdentity<'static>)>(),
                    size_of::<(&salsa::Id, &salsa::Id)>(),
                    size_of::<(u32, u32)>(),
                    size_of::<TypeVarInstance<'static>>(),
                    size_of::<Option<BoundTypeVarInstance<'static>>>(),
                    size_of::<RunResult<Option<BoundTypeVarInstance<'static>>>>(),
                    size_of::<bool>(),
                ],
            ))
        }?;
        let bytes = lookup_bytes + size_of::<Option<(&BoundTypeVarIdentity<'db>, &BoundTypeVarInstance<'db>)>>()
            + size_of::<usize>()
            + size_of::<bool>()
            + scan_bytes;
        self.local_with_fixed_transfers(work + 14 + scan_work, bytes, || {
            let variable = GenericContext::variable_at_in(variables, *cursor);
            if variable.is_some() {
                *cursor += 1;
            }
            variable
        })
        .await
    }

    async fn scope_context(
        &self,
        index: &SemanticIndex<'db>,
        node: BindingNode,
        mode: ReturnCallableTypeVarScope,
    ) -> RunResult<Option<GenericContext<'db>>> {
        scope_context_with(index, node, mode, self).await
    }

    async fn find_in_context(
        &self,
        context: GenericContext<'db>,
        typevar: TypeVarInstance<'db>,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        find_in_context_with(context, typevar, BindingFacts, self).await
    }

    async fn visible(
        &self,
        bound: BoundTypeVarInstance<'db>,
        crossed_class_scope: bool,
    ) -> RunResult<bool> {
        binding_visible_with(bound, crossed_class_scope, BindingFacts, self).await
    }

    async fn find_binding(
        &self,
        index: &SemanticIndex<'db>,
        scope: FileScopeId,
        typevar: TypeVarInstance<'db>,
        mode: ReturnCallableTypeVarScope,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        self.local(1, size_of::<Option<BoundTypeVarInstance<'db>>>(), || ())
            .await?;
        find_typevar_binding_with(self.db(), index, scope, typevar, mode, BindingFacts, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ContextConstructionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Input = ordermap::set::IntoIter<BoundTypeVarInstance<'db>>;

    async fn program(&self, env: &ProgramEnvironment<'db>) -> RunResult<Program<'db>> {
        self.environment_program(env).await
    }

    async fn input_lower_bound(&self, input: &Self::Input) -> RunResult<usize> {
        self.local_with_fixed_transfers(
            3,
            size_of::<(usize, Option<usize>)>(),
            || input.size_hint().0,
        ).await
    }

    async fn next_variable(
        &self,
        input: &mut Self::Input,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        self.local_with_fixed_transfers(
            3,
            size_of::<Option<(BoundTypeVarInstance<'db>, ())>>(),
            || input.next(),
        ).await
    }

    async fn new_variables(&self, lower_bound: usize) -> RunResult<ContextVariables<'db>> {
        let quote = self.local_with_fixed_transfers(
            64,
            64 * (size_of::<usize>() + size_of::<Option<usize>>()) + 8 * size_of::<StorageQuote>(),
            || {
                ordered_merge::<(BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>)>(
                    0, 0, lower_bound,
                )
                .map(|quote| (quote.work, quote.bytes))
                .ok_or(RunError::Contract("generic context allocation quotation overflow"))
            },
        ).await?;
        self.local_quoted_with_fixed_transfers(quote, || {
            FxOrderMap::with_capacity_and_hasher(lower_bound, BuildHasherDefault::default())
        })
        .await
    }

    async fn identity(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarIdentity<'db>> {
        TypeVarBindingEffects::bound_identity(self, variable).await
    }

    async fn insert(
        &self,
        variables: &mut ContextVariables<'db>,
        identity: BoundTypeVarIdentity<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        let quote = self.local_with_fixed_transfers(
            66,
            64 * (size_of::<usize>() + size_of::<Option<usize>>()) + 8 * size_of::<StorageQuote>(),
            || {
                ordered_merge::<(BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>)>(
                    variables.len(), variables.capacity(), 1,
                )
                .map(|quote| (quote.work, quote.bytes))
                .ok_or(RunError::Contract("generic context insertion quotation overflow"))
            },
        ).await?;
        self.local_quoted_with_fixed_transfers(quote, || {
            variables.insert(identity, variable);
        })
        .await?;
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::legacy_context::observe_context_insert(
            self.db(),
        );
        Ok(())
    }

    async fn shrink(&self, variables: &mut ContextVariables<'db>) -> RunResult<()> {
        let quote = self.local_with_fixed_transfers(
            48,
            40 * (size_of::<usize>() + size_of::<Option<usize>>()) + 8 * size_of::<StorageQuote>(),
            || {
                let len = variables.len();
                let capacity = variables.capacity();
                dense_finish::<(BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>)>(len, capacity)
                    .and_then(|entries| {
                        entries.checked_add(dense_finish::<usize>(len, slots(capacity)?)?)
                    })
                    .map(|quote| (quote.work, quote.bytes))
                    .ok_or(RunError::Contract("generic context shrinking quotation overflow"))
            },
        ).await?;
        self.local_quoted_with_fixed_transfers(quote, || variables.shrink_to_fit())
            .await
    }

    async fn intern(
        &self,
        program: Program<'db>,
        variables: ContextVariables<'db>,
    ) -> RunResult<GenericContext<'db>> {
        self.type_parameter_future(|| self.access.intern_generic_context(program, variables))
            .await?
            .await
    }

    async fn publish(&self, context: GenericContext<'db>) -> RunResult<GenericContext<'db>> {
        self.local_with_fixed_transfers(1, 0, || context).await
    }
}

impl<A> sealed::Sealed for SourceEffects<'_, '_, '_, A> {}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeParameterEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn prepare_headers<I>(&self, definitions: &I) -> RunResult<usize>
    where I: ExactSizeIterator<Item = Definition<'db>> + Clone {
        self.local_with_fixed_transfers(2, 0, || definitions.len()).await
    }

    async fn next_definition<I>(&self, definitions: &mut I) -> RunResult<Option<Definition<'db>>>
    where I: Iterator<Item = Definition<'db>> {
        self.local_with_fixed_transfers(3, size_of::<Option<Definition<'db>>>(), || definitions.next()).await
    }

    async fn type_parameter(&self, _db: &'db dyn Db, definition: Definition<'db>) -> RunResult<TypeParameterDeclaration<'db>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        let inference = self.type_parameter_future(|| self.access.definition(definition)).await?.await?;
        let entries = self.local_with_fixed_transfers(4, 0, || match &inference.types {
            DefinitionTypes::Empty | DefinitionTypes::Binding(_) => 0,
            DefinitionTypes::Declaration(_) | DefinitionTypes::BindingAndDeclaration(_) => 1,
            DefinitionTypes::Other(types) => types.declarations.len(),
        }).await?;
        let work = Self::checked(entries.checked_mul(5).and_then(|work| work.checked_add(16)))?;
        let bytes = size_of::<Either<Once<DefinitionDeclaration<'db>>, Copied<slice::Iter<'_, DefinitionDeclaration<'db>>>>>()
            + size_of::<Once<DefinitionDeclaration<'db>>>()
            + size_of::<slice::Iter<'_, DefinitionDeclaration<'db>>>()
            + 2 * size_of::<Option<DefinitionDeclaration<'db>>>()
            + 3 * size_of::<Option<TypeAndQualifiers<'db>>>()
            + size_of::<InferredDeclaration<'db>>()
            + size_of::<Type<'db>>();
        self.local_with_fixed_transfers(work, bytes, || {
            if let Some(declared) = inference.inferred_declaration(definition).declared()
                && let Type::KnownInstance(KnownInstanceType::TypeVar(variable)) = declared.inner_type() {
                TypeParameterDeclaration::Variable(variable)
            } else {
                TypeParameterDeclaration::Rejected
            }
        }).await
    }

    async fn bind(&self, _db: &'db dyn Db, variable: TypeVarInstance<'db>, binding: Definition<'db>) -> RunResult<BoundTypeVarInstance<'db>> {
        self.bind_typevar_in_context(variable, BindingContext::from(binding)).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Builds a PEP 695 context after capturing every parameter's canonical definition.
    /// Each declaration child remains owned by `InferDefinitionTypes`; the context is published
    /// only after every child, binding operation, and ordered-map insertion completes.
    pub(super) async fn pep695_context_source(
        &self,
        index: &SemanticIndex<'db>,
        definition: Definition<'db>,
        parameters: &ast::TypeParams,
    ) -> RunResult<GenericContext<'db>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        let env = self.initialize_value(|| ProgramEnvironment::from_program(self.program)).await?;
        let count = self.local_with_fixed_transfers(1, 0, || parameters.len()).await?;
        let quote = sequence_merge::<Definition<'db>>(0, 0, count)
            .filter(|_| std::alloc::Layout::array::<Definition<'db>>(count).is_ok())
            .ok_or(RunError::Contract("type parameter batch quotation overflow"))?;
        let mut definitions = self.local_with_fixed_transfers(
            Self::checked(quote.work.checked_add(count).and_then(|work| work.checked_add(3)))?,
            quote.bytes,
            || Vec::with_capacity(count),
        ).await?;
        let mut cursor = 0;
        loop {
            let parameter = self.local_with_fixed_transfers(4, size_of::<usize>(), || {
                let parameter = parameters.get(cursor);
                if parameter.is_some() { cursor += 1; }
                parameter
            }).await?;
            let Some(parameter) = parameter else { break; };
            let key = self.local_with_fixed_transfers(3, 0, || match parameter {
                ast::TypeParam::TypeVar(node) => <DefinitionNodeKey as From<&ast::TypeParamTypeVar>>::from(node),
                ast::TypeParam::ParamSpec(node) => <DefinitionNodeKey as From<&ast::TypeParamParamSpec>>::from(node),
                ast::TypeParam::TypeVarTuple(node) => <DefinitionNodeKey as From<&ast::TypeParamTypeVarTuple>>::from(node),
            }).await?;
            let parameter_definition = TypeVarBindingEffects::definition(self, index, key).await?;
            self.local_with_fixed_transfers(3, size_of::<Definition<'db>>(), || definitions.push(parameter_definition)).await?;
        }
        let definition_iter = self.local_with_fixed_transfers(3, 0, || definitions.iter().copied()).await?;
        self.type_parameter_future(|| GenericContext::from_type_param_definitions_with(
            self.db(), &env, definition, definition_iter, self, self,
        )).await?.await
    }
}
