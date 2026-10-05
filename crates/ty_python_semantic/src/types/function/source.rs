//! Source-signature construction shared by inline and controlled inference.

use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast::{self as ast, ParameterWithDefault};
use ty_python_core::definition::Definition;
use ty_python_core::scope::{Scope, ScopeId};
use ty_python_core::{SemanticIndex, semantic_index};

use super::{
    FunctionDecorators, FunctionLiteral, FunctionType, OverloadLiteral, is_implicit_staticmethod,
};
use crate::types::generics::{GenericContext, typing_self};
use crate::types::infer::{nearest_enclosing_class, original_class_type};
use crate::types::signatures::source::{InlineSignatureSourceEffects, SignatureSourceEffects};
use crate::types::signatures::{CallableSignature, ReturnCallableTypeVarScope, Signature};
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, ClassType, KnownClass, SubclassOfInner, SubclassOfType,
    Type, binding_type,
};
use crate::{Db, ProgramEnvironment};

/// Structural source data kept alive while a signature is assembled.
pub(in crate::types) struct FunctionSignatureSource<'db> {
    pub(in crate::types) module: ParsedModuleRef,
    pub(in crate::types) index: &'db SemanticIndex<'db>,
    pub(in crate::types) scope: ScopeId<'db>,
    pub(in crate::types) definition: Definition<'db>,
}

/// Semantic dependencies of a function's source signature.
pub(in crate::types) trait FunctionSignatureEffects<'db>:
    SignatureSourceEffects<'db>
{
    async fn prepare(
        &self,
        db: &'db dyn Db,
        function: OverloadLiteral<'db>,
    ) -> Result<FunctionSignatureSource<'db>, Self::Error>;

    async fn scope_metadata(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
    ) -> Result<&'db Scope, Self::Error>;

    async fn overloads_and_implementation(
        &self,
        db: &'db dyn Db,
        last_definition: OverloadLiteral<'db>,
    ) -> Result<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>), Self::Error>;

    async fn pep695_context(
        &self,
        db: &'db dyn Db,
        index: &SemanticIndex<'db>,
        definition: Definition<'db>,
        type_params: &ast::TypeParams,
    ) -> Result<GenericContext<'db>, Self::Error>;

    async fn class_is_protocol(
        &self,
        db: &'db dyn Db,
        class_definition: Definition<'db>,
    ) -> Result<bool, Self::Error>;

    async fn receiver_method_has_explicit_self(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> Result<bool, Self::Error>;

    async fn original_receiver_class(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<Option<ClassLiteral<'db>>, Self::Error>;

    async fn receiver_class_is_generic(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Self::Error>;

    async fn receiver_class_is_fallback(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Self::Error>;

    async fn synthetic_receiver_self(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, Self::Error>;

    async fn receiver_is_classmethod(
        &self,
        db: &'db dyn Db,
        literal: OverloadLiteral<'db>,
    ) -> Result<bool, Self::Error>;

    async fn receiver_subclass(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: SubclassOfInner<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn receiver_instance(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassLiteral<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn apply_implicit_receiver(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        signature: &mut Signature<'db>,
        receiver: Type<'db>,
    ) -> Result<(), Self::Error>;

    async fn wrap_async_return(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        return_ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn binding_type(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn clone_signature(
        &self,
        db: &'db dyn Db,
        signature: &Signature<'db>,
    ) -> Result<Signature<'db>, Self::Error>;
}

impl<'db> FunctionType<'db> {
    /// Produces the original literal-signature query's value, before updated signatures are selected.
    pub(in crate::types) async fn literal_signature_with<E: FunctionSignatureEffects<'db>>(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<CallableSignature<'db>, E::Error> {
        let literal = effects.field(self.field_requests(db).literal()).await?;
        literal.signature_with(db, effects).await
    }
}

impl<'db> FunctionLiteral<'db> {
    pub(super) async fn signature_with<E: FunctionSignatureEffects<'db>>(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<CallableSignature<'db>, E::Error> {
        let (overloads, implementation) = if self.overloaded {
            effects
                .overloads_and_implementation(db, self.last_definition)
                .await?
        } else {
            (&[][..], Some(self.last_definition))
        };

        // An implementation contributes a public signature only when there are no overloads.
        if let Some(implementation) = implementation
            && overloads.is_empty()
        {
            let mut signature = Some(implementation.signature_with(db, effects).await?);
            return effects
                .local(Some(1), Some(0), || {
                    CallableSignature::from_overloads(signature.take())
                })
                .await;
        }

        let mut signatures = CallableSignature::from_overloads(std::iter::empty());
        for (source_overload_index, overload) in overloads.iter().copied().enumerate() {
            // The last overload may still be inferred, so querying its binding would create a cycle.
            if overload == self.last_definition {
                let signature = overload.signature_with(db, effects).await?;
                push_overload(&mut signatures, signature, source_overload_index, effects).await?;
            } else {
                let source = effects.prepare(db, overload).await?;
                if let Type::Callable(callable) =
                    effects.binding_type(db, source.definition).await?
                {
                    let decorated = effects
                        .field(callable.field_requests(db).signatures())
                        .await?;
                    for signature in &decorated.overloads {
                        let signature = effects.clone_signature(db, signature).await?;
                        push_overload(&mut signatures, signature, source_overload_index, effects)
                            .await?;
                    }
                } else {
                    let signature = overload.signature_with(db, effects).await?;
                    push_overload(&mut signatures, signature, source_overload_index, effects)
                        .await?;
                }
            }
        }
        Ok(signatures)
    }
}

async fn push_overload<'db, E: FunctionSignatureEffects<'db>>(
    signatures: &mut CallableSignature<'db>,
    signature: Signature<'db>,
    source_overload_index: usize,
    effects: &E,
) -> Result<(), E::Error> {
    let mut signature = Some(
        signature
            .with_source_overload_index_with(Some(source_overload_index), effects)
            .await?,
    );
    let required = signatures.overloads.len().checked_add(1);
    let grows = required.is_none_or(|required| required > signatures.overloads.capacity());
    let capacity = if grows {
        required.and_then(usize::checked_next_power_of_two)
    } else {
        Some(signatures.overloads.capacity())
    };
    let requested_bytes = if grows {
        capacity.and_then(|capacity| capacity.checked_mul(size_of::<Signature<'db>>()))
    } else {
        Some(0)
    };
    let work = if grows { required } else { Some(1) };
    effects
        .local(work, requested_bytes, || {
            if let Some(capacity) = capacity {
                signatures
                    .overloads
                    .reserve_exact(capacity - signatures.overloads.len());
            }
            signatures.overloads.extend(signature.take());
        })
        .await
}

impl<'db> OverloadLiteral<'db> {
    pub(in crate::types) async fn signature_with<E: FunctionSignatureEffects<'db>>(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<Signature<'db>, E::Error> {
        let source = effects.prepare(db, self).await?;
        let mut signature = self
            .raw_signature_from_source_with(
                db,
                ReturnCallableTypeVarScope::Public,
                &source,
                effects,
            )
            .await?;
        let scope = effects.scope_metadata(db, source.scope).await?;
        let is_async = effects
            .local(Some(2), Some(0), || {
                scope.node().expect_function().node(&source.module).is_async
            })
            .await?;
        let wrap_return = if is_async {
            let file_scope = effects
                .field(source.scope.read_fields(db).file_scope_id())
                .await?;
            // The frozen set uses binary search over fixed-size scope IDs.
            effects
                .local(Some(usize::BITS as usize + 2), Some(0), || {
                    !file_scope.is_generator_function(source.index)
                })
                .await?
        } else {
            false
        };
        if wrap_return {
            let file = effects
                .field(source.scope.read_fields(db).program_file())
                .await?;
            let env = ProgramEnvironment::from_file(file);
            signature.return_ty = effects
                .wrap_async_return(db, &env, signature.return_ty)
                .await?;
        }
        Ok(signature)
    }

    pub(in crate::types) async fn raw_signature_with<E: FunctionSignatureEffects<'db>>(
        self,
        db: &'db dyn Db,
        return_callable_typevar_scope: ReturnCallableTypeVarScope,
        effects: &E,
    ) -> Result<Signature<'db>, E::Error> {
        let source = effects.prepare(db, self).await?;
        self.raw_signature_from_source_with(db, return_callable_typevar_scope, &source, effects)
            .await
    }

    async fn raw_signature_from_source_with<E: FunctionSignatureEffects<'db>>(
        self,
        db: &'db dyn Db,
        return_callable_typevar_scope: ReturnCallableTypeVarScope,
        source: &FunctionSignatureSource<'db>,
        effects: &E,
    ) -> Result<Signature<'db>, E::Error> {
        let scope = effects.scope_metadata(db, source.scope).await?;
        let node = effects
            .local(Some(1), Some(0), || {
                scope.node().expect_function().node(&source.module)
            })
            .await?;
        let pep695_context = if let Some(type_params) = &node.type_params {
            Some(
                effects
                    .pep695_context(db, source.index, source.definition, type_params)
                    .await?,
            )
        } else {
            None
        };
        let implicit_positional_only = self
            .has_implicitly_positional_only_first_param_with(db, node, source, effects)
            .await?;
        let mut signature = Signature::from_function_with(
            db,
            pep695_context,
            source.definition,
            node,
            implicit_positional_only,
            return_callable_typevar_scope,
            effects,
        )
        .await?;

        let has_unannotated_receiver = effects
            .local(Some(5), Some(0), || {
                signature.parameters().iter().next().is_some_and(|first| {
                    first.is_positional()
                        && first.annotated_type().is_unknown()
                        && first.inferred_annotation
                })
            })
            .await?;
        let receiver_class = if has_unannotated_receiver
            && !self.is_staticmethod_except_new_with(db, effects).await?
        {
            let scope = effects
                .field(source.definition.read_fields(db).scope_id())
                .await?;
            let file_scope = effects.field(scope.read_fields(db).file_scope_id()).await?;
            let class_node = effects
                .local(Some(3), Some(0), || {
                    source.index.scope(file_scope).node().as_class()
                })
                .await?;
            if let Some(class_node) = class_node {
                let work = effects
                    .local(Some(1), Some(0), || source.index.definition_lookup_work())
                    .await?;
                Some(
                    effects
                        .local(work.checked_add(1), Some(0), || {
                            source.index.expect_single_definition(class_node)
                        })
                        .await?,
                )
            } else {
                None
            }
        } else {
            None
        };
        if let Some(class_definition) = receiver_class
            && let Some(receiver) = implicit_receiver_type_with(
                db,
                self,
                source.definition,
                class_definition,
                signature.generic_context,
                effects,
            )
            .await?
        {
            let env = ProgramEnvironment::from_scope(source.scope);
            effects
                .apply_implicit_receiver(db, &env, &mut signature, receiver)
                .await?;
        }
        Ok(signature)
    }

    async fn is_staticmethod_except_new_with<E: FunctionSignatureEffects<'db>>(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<bool, E::Error> {
        let decorators = effects.field(self.field_requests(db).decorators()).await?;
        let explicit = effects
            .local(Some(1), Some(0), || {
                decorators.contains(FunctionDecorators::STATICMETHOD)
            })
            .await?;
        let is_staticmethod = if explicit {
            true
        } else {
            let name = effects.field(self.field_requests(db).name()).await?;
            effects
                .local(Some(8), Some(0), || is_implicit_staticmethod(name))
                .await?
        };
        if is_staticmethod {
            let name = effects.field(self.field_requests(db).name()).await?;
            effects.local(Some(8), Some(0), || name != "__new__").await
        } else {
            Ok(false)
        }
    }

    /// `self` or `cls` can be implicitly positional-only if:
    /// - It is a method AND
    /// - No parameters in the method use PEP-570 syntax AND
    /// - It is not a `@staticmethod` AND
    /// - `self`/`cls` is not explicitly positional-only using the PEP-484 convention AND
    /// - Either the next parameter after `self`/`cls` uses the PEP-484 convention,
    ///   or the enclosing class is a `Protocol` class
    async fn has_implicitly_positional_only_first_param_with<E: FunctionSignatureEffects<'db>>(
        self,
        db: &'db dyn Db,
        node: &ast::StmtFunctionDef,
        source: &FunctionSignatureSource<'db>,
        effects: &E,
    ) -> Result<bool, E::Error> {
        let has_candidate = effects
            .local(Some(5), Some(0), || {
                let parameters = &node.parameters;
                parameters.posonlyargs.is_empty()
                    && parameters
                        .args
                        .first()
                        .is_some_and(|first| !first.uses_pep_484_positional_only_convention())
            })
            .await?;
        if !has_candidate || self.is_staticmethod_except_new_with(db, effects).await? {
            return Ok(false);
        }
        let file_scope = effects
            .field(source.scope.read_fields(db).file_scope_id())
            .await?;
        let work = effects
            .local(Some(1), Some(0), || source.index.definition_lookup_work())
            .await?;
        let class_definition = effects
            .local(work.checked_add(12), Some(0), || {
                source.index.class_definition_of_method(file_scope)
            })
            .await?;
        let Some(class_definition) = class_definition else {
            return Ok(false);
        };

        // `self` and `cls` are always positional-only if the next parameter uses the PEP-484
        // convention. Otherwise, they are implicitly positional-only only in protocols.
        if node
            .parameters
            .args
            .get(1)
            .is_some_and(ParameterWithDefault::uses_pep_484_positional_only_convention)
        {
            return Ok(true);
        }
        effects.class_is_protocol(db, class_definition).await
    }
}

impl<'db> FunctionSignatureEffects<'db> for InlineSignatureSourceEffects {
    async fn prepare(
        &self,
        db: &'db dyn Db,
        function: OverloadLiteral<'db>,
    ) -> Result<FunctionSignatureSource<'db>, Self::Error> {
        let scope = function.body_scope(db);
        let module = parsed_module(db, function.python_file(db)).load(db);
        let index = semantic_index(db, scope.program_file(db));
        let definition = index.expect_single_definition(scope.node(db).expect_function());
        Ok(FunctionSignatureSource {
            module,
            index,
            scope,
            definition,
        })
    }

    async fn scope_metadata(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
    ) -> Result<&'db Scope, Self::Error> {
        Ok(scope.scope(db))
    }

    async fn overloads_and_implementation(
        &self,
        db: &'db dyn Db,
        last_definition: OverloadLiteral<'db>,
    ) -> Result<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>), Self::Error> {
        Ok(FunctionLiteral::overloaded_definitions(db, last_definition))
    }

    async fn pep695_context(
        &self,
        db: &'db dyn Db,
        index: &SemanticIndex<'db>,
        definition: Definition<'db>,
        type_params: &ast::TypeParams,
    ) -> Result<GenericContext<'db>, Self::Error> {
        Ok(GenericContext::from_type_params(
            db,
            index,
            definition,
            type_params,
        ))
    }

    async fn class_is_protocol(
        &self,
        db: &'db dyn Db,
        class_definition: Definition<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(original_class_type(db, class_definition)
            .map(|class_literal| class_literal.default_specialization(db))
            .is_some_and(|class| class.is_protocol(db)))
    }

    async fn receiver_method_has_explicit_self(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(crate::types::signatures::implicit_receiver::context_has_explicit_self(db, context))
    }

    async fn original_receiver_class(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<Option<ClassLiteral<'db>>, Self::Error> {
        Ok(original_class_type(db, definition))
    }

    async fn receiver_class_is_generic(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(class.generic_context(db).is_some())
    }

    async fn receiver_class_is_fallback(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(class.known(db).is_some_and(KnownClass::is_fallback_class))
    }

    async fn synthetic_receiver_self(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, Self::Error> {
        let scope_id = definition.scope(db);
        let typevar_binding_context = Some(definition);
        let index = semantic_index(db, scope_id.program_file(db));
        let class = nearest_enclosing_class(db, index, scope_id).unwrap();
        Ok(
            typing_self(db, scope_id, typevar_binding_context, class.into()).expect(
                "We should always find the surrounding class for an implicit self: Self annotation",
            ),
        )
    }

    async fn receiver_is_classmethod(
        &self,
        db: &'db dyn Db,
        literal: OverloadLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(literal.is_classmethod(db))
    }

    async fn receiver_subclass(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: SubclassOfInner<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(SubclassOfType::from(db, env, receiver))
    }

    async fn receiver_instance(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassLiteral<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(class.to_non_generic_instance(db, env))
    }

    async fn apply_implicit_receiver(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        signature: &mut Signature<'db>,
        receiver: Type<'db>,
    ) -> Result<(), Self::Error> {
        signature.add_implicit_self_annotation(db, env, || Some(receiver));
        Ok(())
    }

    async fn wrap_async_return(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        return_ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(KnownClass::CoroutineType.to_specialized_instance(
            db,
            env,
            &[Type::any(), Type::any(), return_ty],
        ))
    }

    async fn binding_type(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(binding_type(db, definition))
    }

    async fn clone_signature(
        &self,
        _db: &'db dyn Db,
        signature: &Signature<'db>,
    ) -> Result<Signature<'db>, Self::Error> {
        Ok(signature.clone())
    }
}

async fn implicit_receiver_type_with<'db, E: FunctionSignatureEffects<'db>>(
    db: &'db dyn Db,
    literal: OverloadLiteral<'db>,
    definition: Definition<'db>,
    class_definition: Definition<'db>,
    generic_context: Option<GenericContext<'db>>,
    effects: &E,
) -> Result<Option<Type<'db>>, E::Error> {
    let scope = effects
        .field(literal.field_requests(db).body_scope())
        .await?;
    let env = &ProgramEnvironment::from_scope(scope);
    let name = effects.field(literal.field_requests(db).name()).await?;
    let is_dunder_new = effects
        .local(Some(8), Some(0), || name == "__new__")
        .await?;
    // The implicit receiver has not been added yet, so these typevars come from explicit annotations.
    let method_has_explicit_self = if let Some(context) = generic_context {
        effects
            .receiver_method_has_explicit_self(db, context)
            .await?
    } else {
        false
    };
    let Some(class_literal) = effects
        .original_receiver_class(db, class_definition)
        .await?
    else {
        return Ok(None);
    };
    let class_is_generic = effects.receiver_class_is_generic(db, class_literal).await?;
    let class_is_fallback = effects
        .receiver_class_is_fallback(db, class_literal)
        .await?;

    // Normally we implicitly annotate `self` or `cls` with `Self` or `type[Self]`, and
    // create a `Self` typevar that we then have to solve for whenever this method is
    // called. As an optimization, we can skip creating that typevar in certain situations:
    //
    //   - The method cannot use explicit `Self` in any other parameter annotations,
    //     or in its return type. If it does, then we really do need specialization
    //     inference at each call site to see which specific instance type should be
    //     used in those other parameters / return type.
    //
    //   - The class cannot be generic. If it is, then we might need an actual `Self`
    //     typevar to help carry through constraints that relate the instance type to
    //     other typevars in the method signature.
    //
    //   - The class cannot be a "fallback class". A fallback class is used like a mixin,
    //     and so we need specialization inference to determine the "real" class that the
    //     fallback is augmenting. (See KnownClass::is_fallback_class for more details.)
    if method_has_explicit_self || class_is_generic || class_is_fallback {
        let typing_self = effects.synthetic_receiver_self(db, definition).await?;
        if effects.receiver_is_classmethod(db, literal).await? || is_dunder_new {
            effects
                .receiver_subclass(db, env, SubclassOfInner::TypeVar(typing_self))
                .await
                .map(Some)
        } else {
            effects
                .local(Some(1), Some(0), || Some(Type::TypeVar(typing_self)))
                .await
        }
    } else if effects.receiver_is_classmethod(db, literal).await? || is_dunder_new {
        // Without a synthetic typevar, the receiver is the instance or subclass of this class.
        effects
            .receiver_subclass(
                db,
                env,
                SubclassOfInner::Class(ClassType::NonGeneric(class_literal)),
            )
            .await
            .map(Some)
    } else {
        effects
            .receiver_instance(db, env, class_literal)
            .await
            .map(Some)
    }
}
