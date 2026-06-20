use crate::place::Place;
use crate::types::{
    CallArguments, DataclassParams, KnownClass, KnownInstanceType, MemberLookupPolicy,
    SpecialFormType, StaticClassLiteral, SubclassOfType, Type, TypeContext,
    call::CallError,
    callable::CallableFunctionProvenance,
    function::KnownFunction,
    infer::{
        TypeInferenceBuilder,
        builder::{DeclaredAndInferredType, DeferredExpressionState},
        original_class_type,
    },
    special_form::TypeQualifier,
};
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, helpers::any_over_expr};
use ty_module_resolver::{KnownModule, file_to_module};
use ty_python_core::{definition::Definition, scope::NodeWithScopeRef};

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(super) fn infer_class_body(&mut self, class: &ast::StmtClassDef) {
        self.infer_body(&class.body);
    }

    pub(super) fn infer_class_type_params(&mut self, class: &ast::StmtClassDef) {
        let type_params = class
            .type_params
            .as_deref()
            .expect("class type params scope without type params");

        let binding_context = self.index.expect_single_definition(class);
        let previous_typevar_binding_context =
            self.typevar_binding_context.replace(binding_context);

        self.infer_type_parameters(type_params);

        if class.arguments.is_some() {
            let in_stub = self.in_stub();
            let previous_deferred_state =
                std::mem::replace(&mut self.deferred_state, in_stub.into());

            // PEP 695 class headers are inferred in the type-parameter scope, before the completed
            // class type is available. Infer the bases first because `extra_items=T` is an
            // annotation in `class C[T](TypedDict, extra_items=T)`, but an ordinary value argument
            // in `class C[T](Base, extra_items=T)`.
            let mut is_typed_dict = false;

            for base in class.bases() {
                let ty = if let ast::Expr::Starred(starred) = base {
                    let ty = self.infer_expression(&starred.value, TypeContext::default());
                    self.store_expression_type(base, ty);
                    ty
                } else {
                    self.infer_expression(base, TypeContext::default())
                };
                is_typed_dict |= match ty {
                    Type::SpecialForm(SpecialFormType::TypedDict) => true,
                    Type::ClassLiteral(class) => class.is_typed_dict(self.db()),
                    Type::GenericAlias(alias) => alias.is_typed_dict(self.db()),
                    _ => false,
                };
            }

            for keyword in class.keywords() {
                if is_typed_dict && keyword.arg.as_deref() == Some("extra_items") {
                    self.infer_extra_items_kwarg(&keyword.value);
                } else {
                    self.infer_expression(&keyword.value, TypeContext::default());
                }
            }

            self.deferred_state = previous_deferred_state;
        }

        self.typevar_binding_context = previous_typevar_binding_context;
    }

    pub(super) fn infer_class_definition_statement(&mut self, class: &ast::StmtClassDef) {
        self.infer_definition(class);
    }

    pub(super) fn infer_class_definition(
        &mut self,
        class_node: &ast::StmtClassDef,
        definition: Definition<'db>,
    ) {
        let ast::StmtClassDef {
            range: _,
            node_index: _,
            name,
            type_params,
            decorator_list,
            arguments: _,
            body: _,
        } = class_node;
        let db = self.db();

        let mut decorator_types_and_nodes: Vec<(Type<'db>, &ast::Decorator)> =
            Vec::with_capacity(decorator_list.len());
        for decorator in decorator_list {
            let decorator_ty = self.infer_decorator(decorator);
            decorator_types_and_nodes.push((decorator_ty, decorator));
        }

        let body_scope = self
            .index
            .node_scope(NodeWithScopeRef::Class(class_node))
            .to_scope_id(db, self.file());

        let maybe_known_class = KnownClass::try_from_file_and_name(db, self.file(), name);

        let known_module = || file_to_module(db, self.file()).and_then(|module| module.known(db));
        let in_typing_module = || {
            matches!(
                known_module(),
                Some(KnownModule::Typing | KnownModule::TypingExtensions)
            )
        };

        let mut decorators_to_apply = Vec::with_capacity(decorator_types_and_nodes.len());
        let mut metadata_applies_to_original_class = true;
        let mut deprecated = None;
        let mut type_check_only = false;
        let mut dataclass_params = None;
        let mut dataclass_transformer_params = None;
        let mut total_ordering = false;
        let has_explicit_bases = class_node
            .arguments
            .as_deref()
            .is_some_and(|arguments| !arguments.args.is_empty());
        let has_explicit_metaclass = class_node
            .arguments
            .as_deref()
            .is_some_and(|arguments| arguments.find_keyword("metaclass").is_some());
        let infer_original_class_ty = |deprecated,
                                       type_check_only,
                                       dataclass_params,
                                       dataclass_transformer_params,
                                       total_ordering| {
            match (maybe_known_class, &*name.id) {
                (None, "NamedTuple") if in_typing_module() => {
                    Type::SpecialForm(SpecialFormType::NamedTuple)
                }
                (None, "Any") if in_typing_module() => Type::SpecialForm(SpecialFormType::Any),
                (None, "InitVar") if known_module() == Some(KnownModule::Dataclasses) => {
                    Type::SpecialForm(SpecialFormType::TypeQualifier(TypeQualifier::InitVar))
                }
                _ => Type::from(StaticClassLiteral::new(
                    db,
                    name.id.clone(),
                    body_scope,
                    maybe_known_class,
                    deprecated,
                    type_check_only,
                    dataclass_params,
                    dataclass_transformer_params,
                    total_ordering,
                    !class_node.decorator_list.is_empty(),
                    class_node.type_params.is_some(),
                    has_explicit_bases,
                    has_explicit_metaclass,
                )),
            }
        };
        let decorator_call_ty = |decorator: &ast::Decorator| match &decorator.expression {
            ast::Expr::Call(call) => Some(self.expression_type(&call.func)),
            _ => None,
        };

        // In the first pass, collect metadata decorators that shape the original class object.
        // Once an inner decorator replaces the public binding, outer decorators are ordinary
        // runtime applications only: they cannot retroactively add metadata to the original class.
        // For ordinary decorators that still apply to the original class, precompute the call so
        // the second pass can reuse it if no inner decorator has changed the binding.
        for &(decorator_ty, decorator) in decorator_types_and_nodes.iter().rev() {
            if !metadata_applies_to_original_class {
                decorators_to_apply.push((decorator_ty, decorator, None));
                continue;
            }

            if decorator_ty
                .as_function_literal()
                .is_some_and(|function| function.is_known(db, KnownFunction::Dataclass))
            {
                dataclass_params = Some(DataclassParams::default_params(db));
                continue;
            }

            if decorator_ty
                .as_function_literal()
                .is_some_and(|function| function.is_known(db, KnownFunction::TotalOrdering))
            {
                total_ordering = true;
                continue;
            }

            if let Type::DataclassDecorator(params) = decorator_ty {
                dataclass_params = Some(params);
                continue;
            }

            if decorator_ty.is_unknown()
                && let ast::Expr::Call(call) = &decorator.expression
                && self
                    .expression_type(&call.func)
                    .as_function_literal()
                    .is_some_and(|function| function.is_known(db, KnownFunction::Dataclass))
            {
                continue;
            }

            if let Type::KnownInstance(KnownInstanceType::Deprecated(deprecated_inst)) =
                decorator_ty
            {
                deprecated = Some(deprecated_inst);
                continue;
            }

            if decorator_ty
                .as_function_literal()
                .is_some_and(|function| function.is_known(db, KnownFunction::TypeCheckOnly))
            {
                type_check_only = true;
                continue;
            }

            // Skip identity decorators to avoid salsa cycles on typeshed.
            if decorator_ty.as_function_literal().is_some_and(|function| {
                matches!(
                    function.known(db),
                    Some(
                        KnownFunction::Final
                            | KnownFunction::DisjointBase
                            | KnownFunction::RuntimeCheckable
                    )
                )
            }) {
                continue;
            }

            if let Type::FunctionLiteral(f) = decorator_ty {
                // We do not yet detect or flag `@dataclass_transform` applied to more than one
                // overload, or an overload and the implementation both. Nevertheless, this is not
                // allowed. We do not try to treat the offenders intelligently -- just use the
                // params of the last seen usage of `@dataclass_transform`.
                //
                // In class-decorator position, dataclass-transform metadata shapes the
                // original class object. We keep it metadata-only here because the call path
                // uses synthetic dataclass-transform return types to model decorator factories;
                // treating this as an ordinary replacement-returning class decorator would
                // conflate those two cases.
                let transformer_params = f
                    .iter_overloads_and_implementation(db)
                    .rev()
                    .find_map(|overload| overload.dataclass_transformer_params(db));
                if let Some(transformer_params) = transformer_params {
                    dataclass_params = Some(DataclassParams::from_transformer_params(
                        db,
                        transformer_params,
                    ));
                    continue;
                }
            }

            if let Type::DataclassTransformer(params) = decorator_ty {
                dataclass_transformer_params = Some(params);
                continue;
            }

            let original_class_ty = infer_original_class_ty(
                deprecated,
                type_check_only,
                dataclass_params,
                dataclass_transformer_params,
                total_ordering,
            );
            let decorator_result = apply_class_decorator(db, decorator_ty, original_class_ty);
            let decorated_ty = match &decorator_result {
                Ok(return_ty) => *return_ty,
                Err(error) => error.return_type(db),
            };
            if is_unknown_decorator_result(db, decorated_ty) {
                if !preserve_binding_for_unknown_result(
                    db,
                    decorator_ty,
                    decorator_call_ty(decorator),
                    decorated_ty,
                ) {
                    metadata_applies_to_original_class = false;
                }
            } else if !type_retains_original_class(db, original_class_ty, decorated_ty) {
                metadata_applies_to_original_class = false;
            }

            decorators_to_apply.push((
                decorator_ty,
                decorator,
                Some((original_class_ty, decorator_result)),
            ));
        }

        let mut inferred_ty = infer_original_class_ty(
            deprecated,
            type_check_only,
            dataclass_params,
            dataclass_transformer_params,
            total_ordering,
        );

        let original_class_ty = inferred_ty;
        let mut undecorated_ty = None;

        // In the second pass, apply class decorators from inner to outer and use their return types
        // to update the public binding. `original_class_ty` remains the class object whose body and
        // metadata were inferred above.
        for (decorator_ty, decorator_node, precomputed_result) in decorators_to_apply {
            let decorator_result = match precomputed_result {
                // The metadata pass already called this decorator with the same input. If an inner
                // decorator changed the binding, apply this decorator to the new public binding.
                Some((precomputed_input_ty, decorator_result))
                    if precomputed_input_ty == inferred_ty =>
                {
                    decorator_result
                }
                _ => apply_class_decorator(db, decorator_ty, inferred_ty),
            };
            let decorated_ty = match decorator_result {
                Ok(return_ty) => return_ty,
                Err(CallError(_, bindings)) => {
                    bindings.report_diagnostics(&self.context, decorator_node.into());
                    bindings.return_type(db)
                }
            };
            let decorated_ty = match decorated_ty {
                Type::DataclassDecorator(_) | Type::DataclassTransformer(_) => Type::unknown(),
                decorated_ty => decorated_ty,
            };
            // If a class decorator application loses all precision, preserve the original class
            // binding for decorators known to preserve unknown results.
            let should_preserve_binding = is_unknown_decorator_result(db, decorated_ty)
                && preserve_binding_for_unknown_result(
                    db,
                    decorator_ty,
                    decorator_call_ty(decorator_node),
                    decorated_ty,
                );
            inferred_ty = if should_preserve_binding {
                inferred_ty
            } else if class_decorator_preserves_class_binding(db, original_class_ty, decorated_ty) {
                merge_class_preserving_decorator_result(
                    db,
                    original_class_ty,
                    inferred_ty,
                    decorated_ty,
                )
            } else {
                // Only record an undecorated type once a decorator actually replaces the public
                // binding. If all decorators preserve the class, there is no alternate class type
                // to expose.
                undecorated_ty.get_or_insert(inferred_ty);
                decorated_ty
            };
        }

        self.undecorated_type = undecorated_ty;

        self.add_declaration_with_binding(
            class_node.into(),
            definition,
            &DeclaredAndInferredType::are_the_same_type(inferred_ty),
        );

        // if there are type parameters, then the keywords and bases are within that scope
        // and we don't need to run inference here
        if type_params.is_none() {
            // In stub files, keyword values may reference names that are defined later in the file.
            let in_stub = self.in_stub();
            let previous_deferred_state =
                std::mem::replace(&mut self.deferred_state, in_stub.into());
            for keyword in class_node.keywords() {
                if keyword.arg.as_deref() != Some("extra_items") {
                    self.infer_expression(&keyword.value, TypeContext::default());
                }
            }
            self.deferred_state = previous_deferred_state;

            // Inference of bases deferred in stubs, or if any are string literals.
            if self.in_stub()
                || class_node
                    .bases()
                    .iter()
                    .any(|expr| any_over_expr(expr, &ast::Expr::is_string_literal_expr))
                || class_node
                    .arguments
                    .as_deref()
                    .and_then(|args| args.find_keyword("extra_items"))
                    .is_some()
            {
                self.deferred.insert(definition);
            } else {
                let previous_typevar_binding_context =
                    self.typevar_binding_context.replace(definition);
                for base in class_node.bases() {
                    self.infer_expression(base, TypeContext::default());
                }
                self.typevar_binding_context = previous_typevar_binding_context;
            }
        }
    }

    pub(super) fn infer_class_deferred(
        &mut self,
        definition: Definition<'db>,
        class: &ast::StmtClassDef,
    ) {
        let previous_typevar_binding_context = self.typevar_binding_context.replace(definition);
        for base in class.bases() {
            if self.in_stub() {
                self.infer_expression_with_state(
                    base,
                    TypeContext::default(),
                    DeferredExpressionState::Deferred,
                );
            } else {
                self.infer_expression(base, TypeContext::default());
            }
        }

        if let Some(arguments) = class.arguments.as_deref()
            && let Some(extra_items_keyword) = arguments.find_keyword("extra_items")
        {
            if original_class_type(self.db(), definition)
                .is_some_and(|class_literal| class_literal.is_typed_dict(self.db()))
            {
                self.infer_extra_items_kwarg(&extra_items_keyword.value);
            } else if self.in_stub() {
                self.infer_expression_with_state(
                    &extra_items_keyword.value,
                    TypeContext::default(),
                    DeferredExpressionState::Deferred,
                );
            } else {
                self.infer_expression(&extra_items_keyword.value, TypeContext::default());
            }
        }

        self.typevar_binding_context = previous_typevar_binding_context;
    }
}

fn apply_class_decorator<'db>(
    db: &'db dyn crate::Db,
    decorator_ty: Type<'db>,
    decorated_ty: Type<'db>,
) -> Result<Type<'db>, CallError<'db>> {
    let call_arguments = CallArguments::positional([decorated_ty]);
    decorator_ty
        .try_call(db, &call_arguments)
        .map(|bindings| bindings.return_type(db))
}

/// Return true if a decorator result still binds the name to the original class.
///
/// For example, an identity decorator keeps the public name bound to the same class:
/// ```python
/// def identity[T](cls: type[T]) -> type[T]:
///     return cls
///
/// @identity
/// class C: ...
/// ```
///
/// This also accepts metaclass-shaped results such as `type[C]`, because those still describe the
/// original class object even if the decorator call produced a `SubclassOf` type internally.
fn class_decorator_preserves_class_binding<'db>(
    db: &'db dyn crate::Db,
    original_class: Type<'db>,
    decorated_class: Type<'db>,
) -> bool {
    let Type::ClassLiteral(original_literal) = original_class else {
        return false;
    };

    match decorated_class {
        Type::ClassLiteral(decorated_literal) => {
            let decorated_definition = decorated_literal.definition(db);
            decorated_literal == original_literal
                || decorated_definition.is_some()
                    && decorated_definition == original_literal.definition(db)
        }
        Type::SubclassOf(subclass_of) => subclass_of
            .subclass_of()
            .into_class(db)
            .is_some_and(|class| class == original_literal.default_specialization(db)),
        Type::Divergent(_) | Type::Projection(_) => true,
        Type::Union(union) => union
            .elements(db)
            .iter()
            .all(|element| class_decorator_preserves_class_binding(db, original_class, *element)),
        Type::TypeAlias(alias) => {
            class_decorator_preserves_class_binding(db, original_class, alias.value_type(db))
        }
        _ => SubclassOfType::try_from_type(db, original_class).is_some_and(|original_meta_type| {
            decorated_class.is_equivalent_to(db, original_meta_type)
        }),
    }
}

/// Return true if a type still contains the original class object, even if it also carries extra
/// intersection members.
fn type_retains_original_class<'db>(
    db: &'db dyn crate::Db,
    original_class: Type<'db>,
    decorated_class: Type<'db>,
) -> bool {
    match decorated_class {
        Type::Intersection(intersection) => intersection
            .positive(db)
            .iter()
            .any(|element| type_retains_original_class(db, original_class, *element)),
        Type::Union(union) => union
            .elements(db)
            .iter()
            .all(|element| type_retains_original_class(db, original_class, *element)),
        Type::TypeAlias(alias) => {
            type_retains_original_class(db, original_class, alias.value_type(db))
        }
        _ => class_decorator_preserves_class_binding(db, original_class, decorated_class),
    }
}

/// Return true if an unknown class-decorator result should leave the current class type in place.
///
/// This handles both direct decorators and decorator factories:
/// ```python
/// def decorator(cls):
///     return cls
///
/// def decorator_factory():
///     return decorator
///
/// @decorator_factory()
/// class C: ...
/// ```
///
/// The factory case needs the type of the call target, because the type of
/// `@decorator_factory()` is the returned decorator, while the expression type of
/// `decorator_factory` carries the static information that tells us whether an unknown result can
/// be preserved.
fn preserve_binding_for_unknown_result<'db>(
    db: &'db dyn crate::Db,
    decorator_ty: Type<'db>,
    decorator_call_ty: Option<Type<'db>>,
    decorator_result_ty: Type<'db>,
) -> bool {
    ClassDecoratorUnknownResultPolicy::from_decorator(db, decorator_ty, decorator_result_ty)
        == ClassDecoratorUnknownResultPolicy::PreserveBinding
        || decorator_call_ty.is_some_and(|ty| {
            ClassDecoratorUnknownResultPolicy::from_decorator(db, ty, decorator_result_ty)
                == ClassDecoratorUnknownResultPolicy::PreserveBinding
        })
}

/// Return true if applying a class decorator produced no useful replacement type.
fn is_unknown_decorator_result<'db>(db: &'db dyn crate::Db, ty: Type<'db>) -> bool {
    ty.is_unknown() || is_unknown_class_object_decorator_result(db, ty)
}

/// Return true if applying a class decorator produced an unknown class-object type.
///
/// Besides plain `Unknown`, class decorators can produce unknown class-object types such as
/// `type[Any]`. Those are represented as a `SubclassOf` dynamic type, but they should trigger the
/// same preservation fallback as an unknown result:
/// ```python
/// from typing import Any
///
/// def decorator(cls) -> type[Any]: ...
///
/// @decorator
/// class C: ...
/// ```
fn is_unknown_class_object_decorator_result<'db>(db: &'db dyn crate::Db, ty: Type<'db>) -> bool {
    let Type::SubclassOf(subclass_of) = ty.resolve_type_alias(db) else {
        return false;
    };

    subclass_of
        .subclass_of()
        .into_dynamic()
        .is_some_and(|dynamic| Type::Dynamic(dynamic).is_unknown())
}

/// Policy for class decorators whose application result is unknown.
///
/// This is only consulted after applying the decorator produced no useful replacement type. If the
/// decorator itself statically suggests an unannotated identity-preserving shape, we keep the
/// current class binding; if it explicitly promises a replacement type, or if the decorator is
/// unknown, we let the unknown result replace the binding.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum ClassDecoratorUnknownResultPolicy {
    /// Preserve the current class binding when the decorator result is unknown.
    PreserveBinding,
    /// Use the unknown decorator result as the public binding.
    ReplaceBinding,
}

impl ClassDecoratorUnknownResultPolicy {
    /// Infer the unknown-result policy from the decorator's own type.
    ///
    /// Unannotated function and method decorators are treated as class-preserving when their
    /// application result is unknown. Explicit return annotations are trusted as replacement
    /// intent.
    fn from_decorator<'db>(
        db: &'db dyn crate::Db,
        decorator_ty: Type<'db>,
        decorator_result_ty: Type<'db>,
    ) -> Self {
        if decorator_ty.is_unknown() {
            return Self::ReplaceBinding;
        }

        Self::known_from_decorator(db, decorator_ty, decorator_result_ty)
            .unwrap_or(Self::ReplaceBinding)
    }

    /// Return the known preservation policy for a class decorator, if one can be read statically.
    ///
    /// For unknown decorator results, unannotated functions are treated as likely
    /// identity-preserving:
    /// ```python
    /// def decorator(cls):
    ///     return cls
    /// ```
    ///
    /// Explicit return annotations are trusted instead:
    /// ```python
    /// def decorator(cls) -> object:
    ///     return object()
    /// ```
    ///
    /// Callable instances and protocols delegate the decision to their `__call__` member, because
    /// the decorator value itself is not the function that receives the class.
    fn known_from_decorator<'db>(
        db: &'db dyn crate::Db,
        decorator_ty: Type<'db>,
        decorator_result_ty: Type<'db>,
    ) -> Option<Self> {
        match decorator_ty {
            Type::FunctionLiteral(function) => {
                Some(if function.has_explicit_return_annotation(db) {
                    Self::ReplaceBinding
                } else {
                    Self::PreserveBinding
                })
            }
            Type::BoundMethod(method) => {
                Some(if method.function(db).has_explicit_return_annotation(db) {
                    Self::ReplaceBinding
                } else {
                    Self::PreserveBinding
                })
            }
            Type::NominalInstance(_) | Type::ProtocolInstance(_) => {
                let call_symbol = decorator_ty
                    .member_lookup_with_policy(
                        db,
                        Name::new_static("__call__"),
                        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                    )
                    .place;

                if let Place::Defined(place) = call_symbol
                    && place.is_definitely_defined()
                {
                    Some(
                        Self::known_from_decorator(db, place.ty, decorator_result_ty)
                            .unwrap_or(Self::ReplaceBinding),
                    )
                } else {
                    Some(Self::ReplaceBinding)
                }
            }
            Type::Union(union) => Some(
                if union.elements(db).iter().all(|element| {
                    Self::known_from_decorator(db, *element, decorator_result_ty)
                        == Some(Self::PreserveBinding)
                }) {
                    Self::PreserveBinding
                } else {
                    Self::ReplaceBinding
                },
            ),
            Type::TypeAlias(alias) => Some(
                Self::known_from_decorator(db, alias.value_type(db), decorator_result_ty)
                    .unwrap_or(Self::ReplaceBinding),
            ),
            Type::Callable(callable) => Some(match callable.provenance(db) {
                // An unannotated function preserves the class binding when applying it loses the
                // concrete return type:
                // ```python
                // decorator = lambda cls: cls
                //
                // @decorator
                // class C: ...
                // ```
                CallableFunctionProvenance::ImplicitReturn => Self::PreserveBinding,
                // An explicit return annotation can intentionally replace the class binding:
                // ```python
                // def decorator[T](cls) -> T: ...
                //
                // @decorator
                // class C: ...
                // ```
                CallableFunctionProvenance::ExplicitReturn => Self::ReplaceBinding,
                // Generic class-preserving decorator factories can lose the concrete class in
                // their returned `Callable`, while still producing an unknown class-object result:
                // ```python
                // def identity_factory[T]() -> Callable[[type[T]], type[T]]: ...
                //
                // @identity_factory()
                // class C: ...
                // ```
                CallableFunctionProvenance::None
                    if is_unknown_class_object_decorator_result(db, decorator_result_ty) =>
                {
                    Self::PreserveBinding
                }
                // An ordinary `Callable` replacement result has no function provenance to justify
                // the unannotated-function preservation fallback:
                // ```python
                // def replacement_factory[T]() -> Callable[[type[object]], T]: ...
                //
                // @replacement_factory()
                // class C: ...
                // ```
                CallableFunctionProvenance::None => Self::ReplaceBinding,
            }),
            _ => None,
        }
    }
}

/// Merge a class-preserving decorator result into the public binding.
///
/// If earlier decorators already exposed extra members through an intersection, keep those
/// members instead of collapsing back to the undecorated class when a later decorator simply
/// returns the original class object again.
fn merge_class_preserving_decorator_result<'db>(
    db: &'db dyn crate::Db,
    original_class: Type<'db>,
    current_binding: Type<'db>,
    decorated_binding: Type<'db>,
) -> Type<'db> {
    if current_binding == original_class
        || type_retains_original_class(db, original_class, current_binding)
    {
        current_binding
    } else {
        decorated_binding
            .as_class_literal()
            .map(Type::ClassLiteral)
            .unwrap_or(original_class)
    }
}
