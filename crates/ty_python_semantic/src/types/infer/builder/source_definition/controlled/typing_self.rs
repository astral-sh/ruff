//! Bind `Self` from its explicit function body or its original lexical scope.

use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::SemanticIndex;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::node_key::NodeKey;
use ty_python_core::scope::{FileScopeId, NodeWithScopeKey, ScopeId};

use super::class_selection::FixedFieldBorrow;
use super::{FixedFieldCopy, SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::types::class::identity::class_identity_specialization_with;
use crate::types::generics::binding::bind_typevar_with;
use crate::types::generics::typing_self::{TypingSelfEffects, typing_self_with};
use crate::types::local_transfer::generated_field_quote;
use crate::types::typevar::{
    TypeVarBoundOrConstraintsEvaluation, TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance,
};
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, ClassType, Type, TypeVarBoundOrConstraints, TypeVarKind,
    TypeVarVariance,
};

/// Supplies Self metadata and binding scope, optionally validating a caller-provided method body.
struct SourceTypingSelfEffects<'source, 'access, 'run, 'db: 'run, A> {
    source: &'source SourceEffects<'access, 'run, 'db, A>,
    scope: ScopeId<'db>,
    // A method entry must validate its supplied body; general entries resolve the definition's body.
    method_definition: Option<Definition<'db>>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Constructs and binds a class-bounded `Self` for a method.
    ///
    /// `scope` must be the actual body of `method_definition` in the same file. The binding step
    /// verifies the function's node identity: a mismatched or unsupported body is refused with
    /// `TypingSelfBodyScope`, while a different file is a contract error.
    pub(in crate::types::infer) async fn typing_self_for_method(
        &self,
        scope: ScopeId<'db>,
        method_definition: Definition<'db>,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        let (effects, binding) = self
            .local_with_fixed_transfers(10, 0, || {
                (
                    SourceTypingSelfEffects {
                        source: self,
                        scope,
                        method_definition: Some(method_definition),
                    },
                    Some(method_definition),
                )
            })
            .await?;
        self.type_parameter_future(|| typing_self_with(scope, binding, class, &effects))
            .await?
            .await
    }

    /// Constructs class-bounded `Self` using the explicit function binding or lexical scope.
    /// A function definition selects its body through the current semantic index; other
    /// definitions and absent bindings retain the scope supplied by the shared caller.
    pub(in crate::types::infer) async fn typing_self_source(
        &self,
        scope: ScopeId<'db>,
        binding: Option<Definition<'db>>,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        let effects = self
            .local_with_fixed_transfers(6, 0, || SourceTypingSelfEffects {
                source: self,
                scope,
                method_definition: None,
            })
            .await?;
        self.type_parameter_future(|| typing_self_with(scope, binding, class, &effects))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypingSelfEffects<'db>
    for SourceTypingSelfEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn environment(&self, scope: ScopeId<'db>) -> RunResult<ProgramEnvironment<'db>> {
        self.source
            .local_with_fixed_transfers(3, 0, || ProgramEnvironment::from_scope(scope))
            .await
    }

    async fn semantic_index(&self, scope: ScopeId<'db>) -> RunResult<&'db SemanticIndex<'db>> {
        let file = self.source.scope_file(scope).await?;
        self.source.check_file_program(file).await?;
        self.source.access.semantic_index(file).await
    }

    async fn static_name(&self, name: &'static str) -> RunResult<Name> {
        self.source
            .local_with_fixed_transfers(2, 0, || Name::new_static(name))
            .await
    }

    async fn intern_identity(
        &self,
        name: &Name,
        definition: Option<Definition<'db>>,
        kind: TypeVarKind,
    ) -> RunResult<TypeVarIdentity<'db>> {
        // The interner admits its name clone separately; reserve the fields it assembles from it.
        self.source
            .local_with_fixed_transfers(
                4,
                size_of::<(Name, Option<Definition<'db>>, TypeVarKind)>(),
                || (),
            )
            .await?;
        self.source
            .access
            .intern_typevar_identity(name, definition, kind)
            .await
    }

    async fn identity_specialization(&self, class: ClassLiteral<'db>) -> RunResult<ClassType<'db>> {
        match class {
            ClassLiteral::Static(class) => {
                // The shared helper constructs this result after its generic-context child.
                self.source
                    .local_with_fixed_transfers(1, size_of::<ClassType<'db>>(), || ())
                    .await?;
                self.source
                    .type_parameter_future(|| {
                        class_identity_specialization_with(class, self.source)
                    })
                    .await?
                    .await
            }
            ClassLiteral::Dynamic(_)
            | ClassLiteral::DynamicNamedTuple(_)
            | ClassLiteral::DynamicTypedDict(_)
            | ClassLiteral::DynamicEnum(_) => {
                self.source
                    .local_with_fixed_transfers(2, 0, || ClassType::NonGeneric(class))
                    .await
            }
        }
    }

    async fn instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
    ) -> RunResult<Type<'db>> {
        self.source
            .type_parameter_future(|| {
                Type::instance_with(self.source.db(), env, self.source, class)
            })
            .await?
            .await
    }

    async fn upper_bound(
        &self,
        ty: Type<'db>,
    ) -> RunResult<TypeVarBoundOrConstraintsEvaluation<'db>> {
        self.source
            .local_with_fixed_transfers(2, 0, || {
                TypeVarBoundOrConstraintsEvaluation::from(TypeVarBoundOrConstraints::UpperBound(ty))
            })
            .await
    }

    async fn variable_arguments(
        &self,
        bounds: TypeVarBoundOrConstraintsEvaluation<'db>,
        variance: TypeVarVariance,
    ) -> RunResult<(
        Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
        Option<TypeVarVariance>,
    )> {
        self.source
            .local_with_fixed_transfers(3, 0, || (Some(bounds), Some(variance)))
            .await
    }

    async fn intern_variable(
        &self,
        identity: TypeVarIdentity<'db>,
        bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
        variance: Option<TypeVarVariance>,
        default: Option<TypeVarDefaultEvaluation<'db>>,
    ) -> RunResult<TypeVarInstance<'db>> {
        // Reserve the fixed fields tuple before the existing interner helper constructs it.
        self.source
            .local_with_fixed_transfers(
                5,
                size_of::<(
                    TypeVarIdentity<'db>,
                    Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
                    Option<TypeVarVariance>,
                    Option<TypeVarDefaultEvaluation<'db>>,
                )>(),
                || (),
            )
            .await?;
        self.source
            .access
            .intern_typevar_instance(identity, bounds, variance, default)
            .await
    }

    async fn function_node(&self, definition: Definition<'db>) -> RunResult<Option<NodeKey>> {
        let matches = self
            .source
            .local_with_fixed_transfers(3, 0, || {
                self.method_definition
                    .is_none_or(|expected| definition == expected)
            })
            .await?;
        if !matches {
            return self
                .source
                .unavailable(SourceOperation::TypingSelfBodyScope)
                .await;
        }
        let definition_file = self.source.definition_file(definition).await?;
        let scope_file = self.source.scope_file(self.scope).await?;
        self.source.check_file_program(definition_file).await?;
        self.source
            .local_with_fixed_transfers(2, 0, || {
                if definition_file == scope_file {
                    Ok(())
                } else {
                    Err(RunError::Contract(
                        "method Self binding belongs to a different file",
                    ))
                }
            })
            .await??;
        let read = self
            .source
            .boxed_future_with_fixed_transfers(
                generated_field_quote(
                    |definition: Definition<'db>, context| definition.read_fields(context),
                    |definition: Definition<'db>, context| definition.read_fields(context).kind(),
                ),
                || {
                    self.source.access.endpoint().read_field(
                        definition
                            .read_fields(self.source.access.endpoint().field_request_context())
                            .kind(),
                        &FixedFieldBorrow,
                    )
                },
            )
            .await?;
        let kind = read.await;
        self.source
            .local_with_fixed_transfers(2, 0, || match kind {
                DefinitionKind::Function(function) => Some(function.node_key()),
                _ => None,
            })
            .await
    }

    async fn function_scope(
        &self,
        index: &SemanticIndex<'db>,
        function: NodeKey,
    ) -> RunResult<FileScopeId> {
        if self.method_definition.is_none() {
            let quote = self.source.local_with_fixed_transfers(
                24,
                size_of::<usize>() * 12 + size_of::<Option<usize>>() * 12,
                || index.node_scope_lookup_work()
                    .and_then(|work| work.checked_add(8))
                    .map(|work| (work, size_of::<NodeWithScopeKey>() * 2 + size_of::<Option<FileScopeId>>() * 2))
                    .ok_or(RunError::Contract("Self body-scope lookup quotation overflow")),
            ).await?;
            return self
                .source
                .local_quoted_with_fixed_transfers(quote, || {
                    index
                        .try_node_scope_by_key(NodeWithScopeKey::Function(function))
                        .ok_or(RunError::Contract(
                            "Self function definition has no indexed body scope",
                        ))
                })
                .await?;
        }
        let read = self
            .source
            .boxed_future_with_fixed_transfers(
                generated_field_quote(
                    |scope: ScopeId<'db>, context| scope.read_fields(context),
                    |scope: ScopeId<'db>, context| scope.read_fields(context).file_scope_id(),
                ),
                || {
                    self.source.access.endpoint().read_field(
                        self.scope
                            .read_fields(self.source.access.endpoint().field_request_context())
                            .file_scope_id(),
                        &FixedFieldCopy,
                    )
                },
            )
            .await?;
        let file_scope = read.await;
        // The method definition and scope belong to the same file. Comparing their function
        // node indices proves that this is the definition's body, including inside nested classes.
        let containing_scope = self
            .source
            .local_with_fixed_transfers(8, 0, || {
                index
                    .scope(file_scope)
                    .node()
                    .as_function()
                    .and_then(|node| (node.index() == function.index()).then_some(file_scope))
            })
            .await?;
        match containing_scope {
            Some(scope) => Ok(scope),
            None => {
                self.source
                    .unavailable(SourceOperation::TypingSelfBodyScope)
                    .await
            }
        }
    }

    async fn scope_file_scope_id(&self, scope: ScopeId<'db>) -> RunResult<FileScopeId> {
        if self.method_definition.is_some() {
            return self
                .source
                .unavailable(SourceOperation::TypingSelfBodyScope)
                .await;
        }
        let read = self
            .source
            .boxed_future_with_fixed_transfers(
                generated_field_quote(
                    |scope: ScopeId<'db>, context| scope.read_fields(context),
                    |scope: ScopeId<'db>, context| scope.read_fields(context).file_scope_id(),
                ),
                || {
                    self.source.access.endpoint().read_field(
                        scope
                            .read_fields(self.source.access.endpoint().field_request_context())
                            .file_scope_id(),
                        &FixedFieldCopy,
                    )
                },
            )
            .await?;
        Ok(read.await)
    }

    async fn bind(
        &self,
        index: &SemanticIndex<'db>,
        scope: FileScopeId,
        binding: Option<Definition<'db>>,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        // The shared binding helper wraps a found variable in its own optional result.
        self.source
            .local_with_fixed_transfers(1, size_of::<Option<BoundTypeVarInstance<'db>>>(), || ())
            .await?;
        self.source
            .type_parameter_future(|| {
                bind_typevar_with(
                    self.source.db(),
                    index,
                    scope,
                    binding,
                    variable,
                    self.source,
                )
            })
            .await?
            .await
    }
}
