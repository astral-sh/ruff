//! When analyzing a call site, we create _bindings_, which match and type-check the actual
//! arguments against the parameters of the callable. Like with
//! [signatures][crate::types::signatures], we have to handle the fact that the callable might be a
//! union of types, each of which might contain multiple overloads.

use smallvec::SmallVec;

use super::{
    Argument, CallArgumentTypes, CallArguments, CallError, CallErrorKind, CallableSignature,
    InferContext, Signature, Signatures, Type,
};
use crate::db::Db;
use crate::dunder_all::dunder_all_names;
use crate::symbol::{Boundness, Symbol};
use crate::types::diagnostic::{
    CALL_NON_CALLABLE, CONFLICTING_ARGUMENT_FORMS, INVALID_ARGUMENT_TYPE, MISSING_ARGUMENT,
    NO_MATCHING_OVERLOAD, PARAMETER_ALREADY_ASSIGNED, TOO_MANY_POSITIONAL_ARGUMENTS,
    UNKNOWN_ARGUMENT,
};
use crate::types::generics::{Specialization, SpecializationBuilder, SpecializationError};
use crate::types::signatures::{Parameter, ParameterForm};
use crate::types::{
    BoundMethodType, DataclassParams, DataclassTransformerParams, FunctionDecorators, FunctionType,
    KnownClass, KnownFunction, KnownInstanceType, MethodWrapperKind, PropertyInstanceType,
    TupleType, TypeMapping, UnionType, WrapperDescriptorKind, todo_type,
};
use ruff_db::diagnostic::{Annotation, Diagnostic, Severity, SubDiagnostic};
use ruff_python_ast as ast;

/// Binding information for a possible union of callables. At a call site, the arguments must be
/// compatible with _all_ of the types in the union for the call to be valid.
///
/// It's guaranteed that the wrapped bindings have no errors.
#[derive(Debug)]
pub(crate) struct Bindings<'db> {
    signatures: Signatures<'db>,
    /// By using `SmallVec`, we avoid an extra heap allocation for the common case of a non-union
    /// type.
    elements: SmallVec<[CallableBinding<'db>; 1]>,

    /// Whether each argument will be used as a value and/or a type form in this call.
    pub(crate) argument_forms: Box<[Option<ParameterForm>]>,

    conflicting_forms: Box<[bool]>,
}

impl<'db> Bindings<'db> {
    /// Match the arguments of a call site against the parameters of a collection of possibly
    /// unioned, possibly overloaded signatures.
    ///
    /// The returned bindings tell you which parameter (in each signature) each argument was
    /// matched against. You can then perform type inference on each argument with extra context
    /// about the expected parameter types. (You do this by creating a [`CallArgumentTypes`] object
    /// from the `arguments` that you match against.)
    ///
    /// Once you have argument types available, you can call [`check_types`][Self::check_types] to
    /// verify that each argument type is assignable to the corresponding parameter type.
    pub(crate) fn match_parameters(
        signatures: Signatures<'db>,
        arguments: &CallArguments<'_>,
    ) -> Self {
        let mut argument_forms = vec![None; arguments.len()];
        let mut conflicting_forms = vec![false; arguments.len()];
        let elements: SmallVec<[CallableBinding<'db>; 1]> = signatures
            .iter()
            .map(|signature| {
                CallableBinding::match_parameters(
                    signature,
                    arguments,
                    &mut argument_forms,
                    &mut conflicting_forms,
                )
            })
            .collect();

        Bindings {
            signatures,
            elements,
            argument_forms: argument_forms.into(),
            conflicting_forms: conflicting_forms.into(),
        }
    }

    /// Verify that the type of each argument is assignable to type of the parameter that it was
    /// matched to.
    ///
    /// You must provide an `argument_types` that was created from the same `arguments` that you
    /// provided to [`match_parameters`][Self::match_parameters].
    ///
    /// We update the bindings to include the return type of the call, the bound types for all
    /// parameters, and any errors resulting from binding the call, all for each union element and
    /// overload (if any).
    pub(crate) fn check_types(
        mut self,
        db: &'db dyn Db,
        argument_types: &CallArgumentTypes<'_, 'db>,
    ) -> Result<Self, CallError<'db>> {
        for (signature, element) in self.signatures.iter().zip(&mut self.elements) {
            element.check_types(db, signature, argument_types);
        }

        self.evaluate_known_cases(db);

        // In order of precedence:
        //
        // - If every union element is Ok, then the union is too.
        // - If any element has a BindingError, the union has a BindingError.
        // - If every element is NotCallable, then the union is also NotCallable.
        // - Otherwise, the elements are some mixture of Ok, NotCallable, and PossiblyNotCallable.
        //   The union as a whole is PossiblyNotCallable.
        //
        // For example, the union type `Callable[[int], int] | None` may not be callable at all,
        // because the `None` element in this union has no `__call__` method.
        //
        // On the other hand, the union type `Callable[[int], int] | Callable[[str], str]` is
        // always *callable*, but it would produce a `BindingError` if an inhabitant of this type
        // was called with a single `int` argument passed in. That's because the second element in
        // the union doesn't accept an `int` when it's called: it only accepts a `str`.
        let mut all_ok = true;
        let mut any_binding_error = false;
        let mut all_not_callable = true;
        if self.conflicting_forms.contains(&true) {
            all_ok = false;
            any_binding_error = true;
            all_not_callable = false;
        }
        for binding in &self.elements {
            let result = binding.as_result();
            all_ok &= result.is_ok();
            any_binding_error |= matches!(result, Err(CallErrorKind::BindingError));
            all_not_callable &= matches!(result, Err(CallErrorKind::NotCallable));
        }

        if all_ok {
            Ok(self)
        } else if any_binding_error {
            Err(CallError(CallErrorKind::BindingError, Box::new(self)))
        } else if all_not_callable {
            Err(CallError(CallErrorKind::NotCallable, Box::new(self)))
        } else {
            Err(CallError(
                CallErrorKind::PossiblyNotCallable,
                Box::new(self),
            ))
        }
    }

    pub(crate) fn is_single(&self) -> bool {
        self.elements.len() == 1
    }

    pub(crate) fn single_element(&self) -> Option<&CallableBinding<'db>> {
        match self.elements.as_slice() {
            [element] => Some(element),
            _ => None,
        }
    }

    pub(crate) fn callable_type(&self) -> Type<'db> {
        self.signatures.callable_type
    }

    /// Returns the return type of the call. For successful calls, this is the actual return type.
    /// For calls with binding errors, this is a type that best approximates the return type. For
    /// types that are not callable, returns `Type::Unknown`.
    pub(crate) fn return_type(&self, db: &'db dyn Db) -> Type<'db> {
        if let [binding] = self.elements.as_slice() {
            return binding.return_type();
        }
        UnionType::from_elements(db, self.into_iter().map(CallableBinding::return_type))
    }

    /// Report diagnostics for all of the errors that occurred when trying to match actual
    /// arguments to formal parameters. If the callable is a union, or has multiple overloads, we
    /// report a single diagnostic if we couldn't match any union element or overload.
    /// TODO: Update this to add subdiagnostics about how we failed to match each union element and
    /// overload.
    pub(crate) fn report_diagnostics(&self, context: &InferContext<'db>, node: ast::AnyNodeRef) {
        // If all union elements are not callable, report that the union as a whole is not
        // callable.
        if self.into_iter().all(|b| !b.is_callable()) {
            if let Some(builder) = context.report_lint(&CALL_NON_CALLABLE, node) {
                builder.into_diagnostic(format_args!(
                    "Object of type `{}` is not callable",
                    self.callable_type().display(context.db())
                ));
            }
            return;
        }

        for (index, conflicting_form) in self.conflicting_forms.iter().enumerate() {
            if *conflicting_form {
                let node = BindingError::get_node(node, Some(index));
                if let Some(builder) = context.report_lint(&CONFLICTING_ARGUMENT_FORMS, node) {
                    builder.into_diagnostic(
                        "Argument is used as both a value and a type form in call",
                    );
                }
            }
        }

        // If this is not a union, then report a diagnostic for any
        // errors as normal.
        if let Some(binding) = self.single_element() {
            binding.report_diagnostics(context, node, None);
            return;
        }

        for binding in self {
            let union_diag = UnionDiagnostic {
                callable_type: self.callable_type(),
                binding,
            };
            binding.report_diagnostics(context, node, Some(&union_diag));
        }
    }

    /// Evaluates the return type of certain known callables, where we have special-case logic to
    /// determine the return type in a way that isn't directly expressible in the type system.
    fn evaluate_known_cases(&mut self, db: &'db dyn Db) {
        let to_bool = |ty: &Option<Type<'_>>, default: bool| -> bool {
            if let Some(Type::BooleanLiteral(value)) = ty {
                *value
            } else {
                // TODO: emit a diagnostic if we receive `bool`
                default
            }
        };

        // Each special case listed here should have a corresponding clause in `Type::signatures`.
        for (binding, callable_signature) in self.elements.iter_mut().zip(self.signatures.iter()) {
            let binding_type = binding.callable_type;
            for (overload_index, overload) in binding.matching_overloads_mut() {
                match binding_type {
                    Type::MethodWrapper(MethodWrapperKind::FunctionTypeDunderGet(function)) => {
                        if function.has_known_decorator(db, FunctionDecorators::CLASSMETHOD) {
                            match overload.parameter_types() {
                                [_, Some(owner)] => {
                                    overload.set_return_type(Type::BoundMethod(
                                        BoundMethodType::new(db, function, *owner),
                                    ));
                                }
                                [Some(instance), None] => {
                                    overload.set_return_type(Type::BoundMethod(
                                        BoundMethodType::new(
                                            db,
                                            function,
                                            instance.to_meta_type(db),
                                        ),
                                    ));
                                }
                                _ => {}
                            }
                        } else if let [Some(first), _] = overload.parameter_types() {
                            if first.is_none(db) {
                                overload.set_return_type(Type::FunctionLiteral(function));
                            } else {
                                overload.set_return_type(Type::BoundMethod(BoundMethodType::new(
                                    db, function, *first,
                                )));
                            }
                        }
                    }

                    Type::WrapperDescriptor(WrapperDescriptorKind::FunctionTypeDunderGet) => {
                        if let [Some(function_ty @ Type::FunctionLiteral(function)), ..] =
                            overload.parameter_types()
                        {
                            if function.has_known_decorator(db, FunctionDecorators::CLASSMETHOD) {
                                match overload.parameter_types() {
                                    [_, _, Some(owner)] => {
                                        overload.set_return_type(Type::BoundMethod(
                                            BoundMethodType::new(db, *function, *owner),
                                        ));
                                    }

                                    [_, Some(instance), None] => {
                                        overload.set_return_type(Type::BoundMethod(
                                            BoundMethodType::new(
                                                db,
                                                *function,
                                                instance.to_meta_type(db),
                                            ),
                                        ));
                                    }

                                    _ => {}
                                }
                            } else {
                                match overload.parameter_types() {
                                    [_, Some(instance), _] if instance.is_none(db) => {
                                        overload.set_return_type(*function_ty);
                                    }
                                    [_, Some(instance), _] => {
                                        overload.set_return_type(Type::BoundMethod(
                                            BoundMethodType::new(db, *function, *instance),
                                        ));
                                    }

                                    _ => {}
                                }
                            }
                        }
                    }

                    Type::WrapperDescriptor(WrapperDescriptorKind::PropertyDunderGet) => {
                        match overload.parameter_types() {
                            [
                                Some(property @ Type::PropertyInstance(_)),
                                Some(instance),
                                ..,
                            ] if instance.is_none(db) => {
                                overload.set_return_type(*property);
                            }
                            [
                                Some(Type::PropertyInstance(property)),
                                Some(Type::KnownInstance(KnownInstanceType::TypeAliasType(
                                    type_alias,
                                ))),
                                ..,
                            ] if property.getter(db).is_some_and(|getter| {
                                getter
                                    .into_function_literal()
                                    .is_some_and(|f| f.name(db) == "__name__")
                            }) =>
                            {
                                overload
                                    .set_return_type(Type::string_literal(db, type_alias.name(db)));
                            }
                            [
                                Some(Type::PropertyInstance(property)),
                                Some(Type::KnownInstance(KnownInstanceType::TypeVar(typevar))),
                                ..,
                            ] => {
                                match property
                                    .getter(db)
                                    .and_then(Type::into_function_literal)
                                    .map(|f| f.name(db).as_str())
                                {
                                    Some("__name__") => {
                                        overload.set_return_type(Type::string_literal(
                                            db,
                                            typevar.name(db),
                                        ));
                                    }
                                    Some("__bound__") => {
                                        overload.set_return_type(
                                            typevar
                                                .upper_bound(db)
                                                .unwrap_or_else(|| Type::none(db)),
                                        );
                                    }
                                    Some("__constraints__") => {
                                        overload.set_return_type(TupleType::from_elements(
                                            db,
                                            typevar.constraints(db).into_iter().flatten(),
                                        ));
                                    }
                                    Some("__default__") => {
                                        overload.set_return_type(
                                            typevar.default_ty(db).unwrap_or_else(|| {
                                                KnownClass::NoDefaultType.to_instance(db)
                                            }),
                                        );
                                    }
                                    _ => {}
                                }
                            }
                            [Some(Type::PropertyInstance(property)), Some(instance), ..] => {
                                if let Some(getter) = property.getter(db) {
                                    if let Ok(return_ty) = getter
                                        .try_call(db, &CallArgumentTypes::positional([*instance]))
                                        .map(|binding| binding.return_type(db))
                                    {
                                        overload.set_return_type(return_ty);
                                    } else {
                                        overload.errors.push(BindingError::InternalCallError(
                                            "calling the getter failed",
                                        ));
                                        overload.set_return_type(Type::unknown());
                                    }
                                } else {
                                    overload.errors.push(BindingError::InternalCallError(
                                        "property has no getter",
                                    ));
                                    overload.set_return_type(Type::Never);
                                }
                            }
                            _ => {}
                        }
                    }

                    Type::MethodWrapper(MethodWrapperKind::PropertyDunderGet(property)) => {
                        match overload.parameter_types() {
                            [Some(instance), ..] if instance.is_none(db) => {
                                overload.set_return_type(Type::PropertyInstance(property));
                            }
                            [Some(instance), ..] => {
                                if let Some(getter) = property.getter(db) {
                                    if let Ok(return_ty) = getter
                                        .try_call(db, &CallArgumentTypes::positional([*instance]))
                                        .map(|binding| binding.return_type(db))
                                    {
                                        overload.set_return_type(return_ty);
                                    } else {
                                        overload.errors.push(BindingError::InternalCallError(
                                            "calling the getter failed",
                                        ));
                                        overload.set_return_type(Type::unknown());
                                    }
                                } else {
                                    overload.set_return_type(Type::Never);
                                    overload.errors.push(BindingError::InternalCallError(
                                        "property has no getter",
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }

                    Type::WrapperDescriptor(WrapperDescriptorKind::PropertyDunderSet) => {
                        if let [
                            Some(Type::PropertyInstance(property)),
                            Some(instance),
                            Some(value),
                            ..,
                        ] = overload.parameter_types()
                        {
                            if let Some(setter) = property.setter(db) {
                                if let Err(_call_error) = setter.try_call(
                                    db,
                                    &CallArgumentTypes::positional([*instance, *value]),
                                ) {
                                    overload.errors.push(BindingError::InternalCallError(
                                        "calling the setter failed",
                                    ));
                                }
                            } else {
                                overload.errors.push(BindingError::InternalCallError(
                                    "property has no setter",
                                ));
                            }
                        }
                    }

                    Type::MethodWrapper(MethodWrapperKind::PropertyDunderSet(property)) => {
                        if let [Some(instance), Some(value), ..] = overload.parameter_types() {
                            if let Some(setter) = property.setter(db) {
                                if let Err(_call_error) = setter.try_call(
                                    db,
                                    &CallArgumentTypes::positional([*instance, *value]),
                                ) {
                                    overload.errors.push(BindingError::InternalCallError(
                                        "calling the setter failed",
                                    ));
                                }
                            } else {
                                overload.errors.push(BindingError::InternalCallError(
                                    "property has no setter",
                                ));
                            }
                        }
                    }

                    Type::MethodWrapper(MethodWrapperKind::StrStartswith(literal)) => {
                        if let [Some(Type::StringLiteral(prefix)), None, None] =
                            overload.parameter_types()
                        {
                            overload.set_return_type(Type::BooleanLiteral(
                                literal.value(db).starts_with(prefix.value(db)),
                            ));
                        }
                    }

                    Type::DataclassTransformer(params) => {
                        if let [Some(Type::FunctionLiteral(function))] = overload.parameter_types()
                        {
                            overload.set_return_type(Type::FunctionLiteral(
                                function.with_dataclass_transformer_params(db, params),
                            ));
                        }
                    }

                    Type::BoundMethod(bound_method)
                        if bound_method.self_instance(db).is_property_instance() =>
                    {
                        match bound_method.function(db).name(db).as_str() {
                            "setter" => {
                                if let [Some(_), Some(setter)] = overload.parameter_types() {
                                    let mut ty_property = bound_method.self_instance(db);
                                    if let Type::PropertyInstance(property) = ty_property {
                                        ty_property =
                                            Type::PropertyInstance(PropertyInstanceType::new(
                                                db,
                                                property.getter(db),
                                                Some(*setter),
                                            ));
                                    }
                                    overload.set_return_type(ty_property);
                                }
                            }
                            "getter" => {
                                if let [Some(_), Some(getter)] = overload.parameter_types() {
                                    let mut ty_property = bound_method.self_instance(db);
                                    if let Type::PropertyInstance(property) = ty_property {
                                        ty_property =
                                            Type::PropertyInstance(PropertyInstanceType::new(
                                                db,
                                                Some(*getter),
                                                property.setter(db),
                                            ));
                                    }
                                    overload.set_return_type(ty_property);
                                }
                            }
                            "deleter" => {
                                // TODO: we do not store deleters yet
                                let ty_property = bound_method.self_instance(db);
                                overload.set_return_type(ty_property);
                            }
                            _ => {
                                // Fall back to typeshed stubs for all other methods
                            }
                        }
                    }

                    Type::FunctionLiteral(function_type) => match function_type.known(db) {
                        Some(KnownFunction::IsEquivalentTo) => {
                            if let [Some(ty_a), Some(ty_b)] = overload.parameter_types() {
                                overload.set_return_type(Type::BooleanLiteral(
                                    ty_a.is_equivalent_to(db, *ty_b),
                                ));
                            }
                        }

                        Some(KnownFunction::IsSubtypeOf) => {
                            if let [Some(ty_a), Some(ty_b)] = overload.parameter_types() {
                                overload.set_return_type(Type::BooleanLiteral(
                                    ty_a.is_subtype_of(db, *ty_b),
                                ));
                            }
                        }

                        Some(KnownFunction::IsAssignableTo) => {
                            if let [Some(ty_a), Some(ty_b)] = overload.parameter_types() {
                                overload.set_return_type(Type::BooleanLiteral(
                                    ty_a.is_assignable_to(db, *ty_b),
                                ));
                            }
                        }

                        Some(KnownFunction::IsDisjointFrom) => {
                            if let [Some(ty_a), Some(ty_b)] = overload.parameter_types() {
                                overload.set_return_type(Type::BooleanLiteral(
                                    ty_a.is_disjoint_from(db, *ty_b),
                                ));
                            }
                        }

                        Some(KnownFunction::IsGradualEquivalentTo) => {
                            if let [Some(ty_a), Some(ty_b)] = overload.parameter_types() {
                                overload.set_return_type(Type::BooleanLiteral(
                                    ty_a.is_gradual_equivalent_to(db, *ty_b),
                                ));
                            }
                        }

                        Some(KnownFunction::IsFullyStatic) => {
                            if let [Some(ty)] = overload.parameter_types() {
                                overload
                                    .set_return_type(Type::BooleanLiteral(ty.is_fully_static(db)));
                            }
                        }

                        Some(KnownFunction::IsSingleton) => {
                            if let [Some(ty)] = overload.parameter_types() {
                                overload.set_return_type(Type::BooleanLiteral(ty.is_singleton(db)));
                            }
                        }

                        Some(KnownFunction::IsSingleValued) => {
                            if let [Some(ty)] = overload.parameter_types() {
                                overload
                                    .set_return_type(Type::BooleanLiteral(ty.is_single_valued(db)));
                            }
                        }

                        Some(KnownFunction::GenericContext) => {
                            if let [Some(ty)] = overload.parameter_types() {
                                // TODO: Handle generic functions, and unions/intersections of
                                // generic types
                                overload.set_return_type(match ty {
                                    Type::ClassLiteral(class) => match class.generic_context(db) {
                                        Some(generic_context) => TupleType::from_elements(
                                            db,
                                            generic_context
                                                .variables(db)
                                                .iter()
                                                .map(|typevar| Type::TypeVar(*typevar)),
                                        ),
                                        None => Type::none(db),
                                    },

                                    _ => Type::none(db),
                                });
                            }
                        }

                        Some(KnownFunction::DunderAllNames) => {
                            if let [Some(ty)] = overload.parameter_types() {
                                overload.set_return_type(match ty {
                                    Type::ModuleLiteral(module_literal) => {
                                        let all_names = module_literal
                                            .module(db)
                                            .file()
                                            .map(|file| dunder_all_names(db, file))
                                            .unwrap_or_default();
                                        match all_names {
                                            Some(names) => {
                                                let mut names = names.iter().collect::<Vec<_>>();
                                                names.sort();
                                                TupleType::from_elements(
                                                    db,
                                                    names.iter().map(|name| {
                                                        Type::string_literal(db, name.as_str())
                                                    }),
                                                )
                                            }
                                            None => Type::none(db),
                                        }
                                    }
                                    _ => Type::none(db),
                                });
                            }
                        }

                        Some(KnownFunction::Len) => {
                            if let [Some(first_arg)] = overload.parameter_types() {
                                if let Some(len_ty) = first_arg.len(db) {
                                    overload.set_return_type(len_ty);
                                }
                            }
                        }

                        Some(KnownFunction::Repr) => {
                            if let [Some(first_arg)] = overload.parameter_types() {
                                overload.set_return_type(first_arg.repr(db));
                            }
                        }

                        Some(KnownFunction::Cast) => {
                            if let [Some(casted_ty), Some(_)] = overload.parameter_types() {
                                overload.set_return_type(*casted_ty);
                            }
                        }

                        Some(KnownFunction::IsProtocol) => {
                            if let [Some(ty)] = overload.parameter_types() {
                                overload.set_return_type(Type::BooleanLiteral(
                                    ty.into_class_literal()
                                        .is_some_and(|class| class.is_protocol(db)),
                                ));
                            }
                        }

                        Some(KnownFunction::GetProtocolMembers) => {
                            if let [Some(Type::ClassLiteral(class))] = overload.parameter_types() {
                                if let Some(protocol_class) = class.into_protocol_class(db) {
                                    let member_names = protocol_class
                                        .interface(db)
                                        .members(db)
                                        .map(|member| Type::string_literal(db, member.name()));
                                    let specialization = UnionType::from_elements(db, member_names);
                                    overload.set_return_type(
                                        KnownClass::FrozenSet
                                            .to_specialized_instance(db, [specialization]),
                                    );
                                }
                            }
                        }

                        Some(KnownFunction::Overload) => {
                            // TODO: This can be removed once we understand legacy generics because the
                            // typeshed definition for `typing.overload` is an identity function.
                            if let [Some(ty)] = overload.parameter_types() {
                                overload.set_return_type(*ty);
                            }
                        }

                        Some(KnownFunction::Override) => {
                            // TODO: This can be removed once we understand legacy generics because the
                            // typeshed definition for `typing.overload` is an identity function.
                            if let [Some(ty)] = overload.parameter_types() {
                                overload.set_return_type(*ty);
                            }
                        }

                        Some(KnownFunction::AbstractMethod) => {
                            // TODO: This can be removed once we understand legacy generics because the
                            // typeshed definition for `abc.abstractmethod` is an identity function.
                            if let [Some(ty)] = overload.parameter_types() {
                                overload.set_return_type(*ty);
                            }
                        }

                        Some(KnownFunction::Final) => {
                            // TODO: This can be removed once we understand legacy generics because the
                            // typeshed definition for `abc.abstractmethod` is an identity function.
                            if let [Some(ty)] = overload.parameter_types() {
                                overload.set_return_type(*ty);
                            }
                        }

                        Some(KnownFunction::GetattrStatic) => {
                            let [Some(instance_ty), Some(attr_name), default] =
                                overload.parameter_types()
                            else {
                                continue;
                            };

                            let Some(attr_name) = attr_name.into_string_literal() else {
                                continue;
                            };

                            let default = if let Some(default) = default {
                                *default
                            } else {
                                Type::Never
                            };

                            let union_with_default =
                                |ty| UnionType::from_elements(db, [ty, default]);

                            // TODO: we could emit a diagnostic here (if default is not set)
                            overload.set_return_type(
                                match instance_ty.static_member(db, attr_name.value(db)) {
                                    Symbol::Type(ty, Boundness::Bound) => {
                                        if instance_ty.is_fully_static(db) {
                                            ty
                                        } else {
                                            // Here, we attempt to model the fact that an attribute lookup on
                                            // a non-fully static type could fail. This is an approximation,
                                            // as there are gradual types like `tuple[Any]`, on which a lookup
                                            // of (e.g. of the `index` method) would always succeed.

                                            union_with_default(ty)
                                        }
                                    }
                                    Symbol::Type(ty, Boundness::PossiblyUnbound) => {
                                        union_with_default(ty)
                                    }
                                    Symbol::Unbound => default,
                                },
                            );
                        }

                        Some(KnownFunction::Dataclass) => {
                            if let [
                                init,
                                repr,
                                eq,
                                order,
                                unsafe_hash,
                                frozen,
                                match_args,
                                kw_only,
                                slots,
                                weakref_slot,
                            ] = overload.parameter_types()
                            {
                                let mut params = DataclassParams::empty();

                                if to_bool(init, true) {
                                    params |= DataclassParams::INIT;
                                }
                                if to_bool(repr, true) {
                                    params |= DataclassParams::REPR;
                                }
                                if to_bool(eq, true) {
                                    params |= DataclassParams::EQ;
                                }
                                if to_bool(order, false) {
                                    params |= DataclassParams::ORDER;
                                }
                                if to_bool(unsafe_hash, false) {
                                    params |= DataclassParams::UNSAFE_HASH;
                                }
                                if to_bool(frozen, false) {
                                    params |= DataclassParams::FROZEN;
                                }
                                if to_bool(match_args, true) {
                                    params |= DataclassParams::MATCH_ARGS;
                                }
                                if to_bool(kw_only, false) {
                                    params |= DataclassParams::KW_ONLY;
                                }
                                if to_bool(slots, false) {
                                    params |= DataclassParams::SLOTS;
                                }
                                if to_bool(weakref_slot, false) {
                                    params |= DataclassParams::WEAKREF_SLOT;
                                }

                                overload.set_return_type(Type::DataclassDecorator(params));
                            }
                        }

                        Some(KnownFunction::DataclassTransform) => {
                            if let [
                                eq_default,
                                order_default,
                                kw_only_default,
                                frozen_default,
                                _field_specifiers,
                                _kwargs,
                            ] = overload.parameter_types()
                            {
                                let mut params = DataclassTransformerParams::empty();

                                if to_bool(eq_default, true) {
                                    params |= DataclassTransformerParams::EQ_DEFAULT;
                                }
                                if to_bool(order_default, false) {
                                    params |= DataclassTransformerParams::ORDER_DEFAULT;
                                }
                                if to_bool(kw_only_default, false) {
                                    params |= DataclassTransformerParams::KW_ONLY_DEFAULT;
                                }
                                if to_bool(frozen_default, false) {
                                    params |= DataclassTransformerParams::FROZEN_DEFAULT;
                                }

                                overload.set_return_type(Type::DataclassTransformer(params));
                            }
                        }

                        _ => {
                            let mut handle_dataclass_transformer_params =
                                |function_type: &FunctionType| {
                                    if let Some(params) =
                                        function_type.dataclass_transformer_params(db)
                                    {
                                        // This is a call to a custom function that was decorated with `@dataclass_transformer`.
                                        // If this function was called with a keyword argument like `order=False`, we extract
                                        // the argument type and overwrite the corresponding flag in `dataclass_params` after
                                        // constructing them from the `dataclass_transformer`-parameter defaults.

                                        let mut dataclass_params = DataclassParams::from(params);

                                        if let Some(Some(Type::BooleanLiteral(order))) =
                                            callable_signature.iter().nth(overload_index).and_then(
                                                |signature| {
                                                    let (idx, _) = signature
                                                        .parameters()
                                                        .keyword_by_name("order")?;
                                                    overload.parameter_types().get(idx)
                                                },
                                            )
                                        {
                                            dataclass_params.set(DataclassParams::ORDER, *order);
                                        }

                                        overload.set_return_type(Type::DataclassDecorator(
                                            dataclass_params,
                                        ));
                                    }
                                };

                            // Ideally, either the implementation, or exactly one of the overloads
                            // of the function can have the dataclass_transform decorator applied.
                            // However, we do not yet enforce this, and in the case of multiple
                            // applications of the decorator, we will only consider the last one
                            // for the return value, since the prior ones will be over-written.
                            if let Some(overloaded) = function_type.to_overloaded(db) {
                                overloaded
                                    .overloads
                                    .iter()
                                    .for_each(&mut handle_dataclass_transformer_params);
                            }

                            handle_dataclass_transformer_params(&function_type);
                        }
                    },

                    Type::ClassLiteral(class) => match class.known(db) {
                        Some(KnownClass::Bool) => match overload.parameter_types() {
                            [Some(arg)] => overload.set_return_type(arg.bool(db).into_type(db)),
                            [None] => overload.set_return_type(Type::BooleanLiteral(false)),
                            _ => {}
                        },

                        Some(KnownClass::Str) if overload_index == 0 => {
                            match overload.parameter_types() {
                                [Some(arg)] => overload.set_return_type(arg.str(db)),
                                [None] => overload.set_return_type(Type::string_literal(db, "")),
                                _ => {}
                            }
                        }

                        Some(KnownClass::Type) if overload_index == 0 => {
                            if let [Some(arg)] = overload.parameter_types() {
                                overload.set_return_type(arg.to_meta_type(db));
                            }
                        }

                        Some(KnownClass::Property) => {
                            if let [getter, setter, ..] = overload.parameter_types() {
                                overload.set_return_type(Type::PropertyInstance(
                                    PropertyInstanceType::new(db, *getter, *setter),
                                ));
                            }
                        }

                        _ => {}
                    },

                    Type::KnownInstance(KnownInstanceType::TypedDict) => {
                        overload.set_return_type(todo_type!("TypedDict"));
                    }

                    // Not a special case
                    _ => {}
                }
            }
        }
    }
}

impl<'a, 'db> IntoIterator for &'a Bindings<'db> {
    type Item = &'a CallableBinding<'db>;
    type IntoIter = std::slice::Iter<'a, CallableBinding<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.elements.iter()
    }
}

impl<'a, 'db> IntoIterator for &'a mut Bindings<'db> {
    type Item = &'a mut CallableBinding<'db>;
    type IntoIter = std::slice::IterMut<'a, CallableBinding<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.elements.iter_mut()
    }
}

/// Binding information for a single callable. If the callable is overloaded, there is a separate
/// [`Binding`] for each overload.
///
/// For a successful binding, each argument is mapped to one of the callable's formal parameters.
/// If the callable has multiple overloads, the first one that matches is used as the overall
/// binding match.
///
/// TODO: Implement the call site evaluation algorithm in the [proposed updated typing
/// spec][overloads], which is much more subtle than “first match wins”.
///
/// If the arguments cannot be matched to formal parameters, we store information about the
/// specific errors that occurred when trying to match them up. If the callable has multiple
/// overloads, we store this error information for each overload.
///
/// [overloads]: https://github.com/python/typing/pull/1839
#[derive(Debug)]
pub(crate) struct CallableBinding<'db> {
    pub(crate) callable_type: Type<'db>,
    pub(crate) signature_type: Type<'db>,
    pub(crate) dunder_call_is_possibly_unbound: bool,

    /// The bindings of each overload of this callable. Will be empty if the type is not callable.
    ///
    /// By using `SmallVec`, we avoid an extra heap allocation for the common case of a
    /// non-overloaded callable.
    overloads: SmallVec<[Binding<'db>; 1]>,
}

impl<'db> CallableBinding<'db> {
    fn match_parameters(
        signature: &CallableSignature<'db>,
        arguments: &CallArguments<'_>,
        argument_forms: &mut [Option<ParameterForm>],
        conflicting_forms: &mut [bool],
    ) -> Self {
        // If this callable is a bound method, prepend the self instance onto the arguments list
        // before checking.
        let arguments = arguments.with_self(signature.bound_type);

        // TODO: This checks every overload. In the proposed more detailed call checking spec [1],
        // arguments are checked for arity first, and are only checked for type assignability against
        // the matching overloads. Make sure to implement that as part of separating call binding into
        // two phases.
        //
        // [1] https://typing.python.org/en/latest/spec/overload.html#overload-call-evaluation
        let overloads = signature
            .into_iter()
            .map(|signature| {
                Binding::match_parameters(
                    signature,
                    arguments.as_ref(),
                    argument_forms,
                    conflicting_forms,
                )
            })
            .collect();

        CallableBinding {
            callable_type: signature.callable_type,
            signature_type: signature.signature_type,
            dunder_call_is_possibly_unbound: signature.dunder_call_is_possibly_unbound,
            overloads,
        }
    }

    fn check_types(
        &mut self,
        db: &'db dyn Db,
        signature: &CallableSignature<'db>,
        argument_types: &CallArgumentTypes<'_, 'db>,
    ) {
        // If this callable is a bound method, prepend the self instance onto the arguments list
        // before checking.
        let argument_types = argument_types.with_self(signature.bound_type);
        for (signature, overload) in signature.iter().zip(&mut self.overloads) {
            overload.check_types(db, signature, argument_types.as_ref());
        }
    }

    fn as_result(&self) -> Result<(), CallErrorKind> {
        if !self.is_callable() {
            return Err(CallErrorKind::NotCallable);
        }

        if self.has_binding_errors() {
            return Err(CallErrorKind::BindingError);
        }

        if self.dunder_call_is_possibly_unbound {
            return Err(CallErrorKind::PossiblyNotCallable);
        }

        Ok(())
    }

    fn is_callable(&self) -> bool {
        !self.overloads.is_empty()
    }

    /// Returns whether there were any errors binding this call site. If the callable has multiple
    /// overloads, they must _all_ have errors.
    pub(crate) fn has_binding_errors(&self) -> bool {
        self.matching_overloads().next().is_none()
    }

    /// Returns an iterator over all the overloads that matched for this call binding.
    pub(crate) fn matching_overloads(&self) -> impl Iterator<Item = (usize, &Binding<'db>)> {
        self.overloads
            .iter()
            .enumerate()
            .filter(|(_, overload)| overload.as_result().is_ok())
    }

    /// Returns an iterator over all the mutable overloads that matched for this call binding.
    pub(crate) fn matching_overloads_mut(
        &mut self,
    ) -> impl Iterator<Item = (usize, &mut Binding<'db>)> {
        self.overloads
            .iter_mut()
            .enumerate()
            .filter(|(_, overload)| overload.as_result().is_ok())
    }

    /// Returns the return type of this call. For a valid call, this is the return type of the
    /// first overload that the arguments matched against. For an invalid call to a non-overloaded
    /// function, this is the return type of the function. For an invalid call to an overloaded
    /// function, we return `Type::unknown`, since we cannot make any useful conclusions about
    /// which overload was intended to be called.
    pub(crate) fn return_type(&self) -> Type<'db> {
        // TODO: Implement the overload call evaluation algorithm as mentioned in the spec [1] to
        // get the matching overload and use that to get the return type.
        //
        // [1]: https://typing.python.org/en/latest/spec/overload.html#overload-call-evaluation
        if let Some((_, first_overload)) = self.matching_overloads().next() {
            return first_overload.return_type();
        }
        if let [overload] = self.overloads.as_slice() {
            return overload.return_type();
        }
        Type::unknown()
    }

    fn report_diagnostics(
        &self,
        context: &InferContext<'db>,
        node: ast::AnyNodeRef,
        union_diag: Option<&UnionDiagnostic<'_, '_>>,
    ) {
        if !self.is_callable() {
            if let Some(builder) = context.report_lint(&CALL_NON_CALLABLE, node) {
                let mut diag = builder.into_diagnostic(format_args!(
                    "Object of type `{}` is not callable",
                    self.callable_type.display(context.db()),
                ));
                if let Some(union_diag) = union_diag {
                    union_diag.add_union_context(context.db(), &mut diag);
                }
            }
            return;
        }

        if self.dunder_call_is_possibly_unbound {
            if let Some(builder) = context.report_lint(&CALL_NON_CALLABLE, node) {
                let mut diag = builder.into_diagnostic(format_args!(
                    "Object of type `{}` is not callable (possibly unbound `__call__` method)",
                    self.callable_type.display(context.db()),
                ));
                if let Some(union_diag) = union_diag {
                    union_diag.add_union_context(context.db(), &mut diag);
                }
            }
            return;
        }

        match self.overloads.as_slice() {
            [] => {}
            [overload] => {
                let callable_description =
                    CallableDescription::new(context.db(), self.signature_type);
                overload.report_diagnostics(
                    context,
                    node,
                    self.signature_type,
                    callable_description.as_ref(),
                    union_diag,
                );
            }
            _overloads => {
                // When the number of unmatched overloads exceeds this number, we stop
                // printing them to avoid excessive output.
                //
                // An example of a routine with many many overloads:
                // https://github.com/henribru/google-api-python-client-stubs/blob/master/googleapiclient-stubs/discovery.pyi
                const MAXIMUM_OVERLOADS: usize = 50;

                let Some(builder) = context.report_lint(&NO_MATCHING_OVERLOAD, node) else {
                    return;
                };
                let callable_description =
                    CallableDescription::new(context.db(), self.callable_type);
                let mut diag = builder.into_diagnostic(format_args!(
                    "No overload{} matches arguments",
                    if let Some(CallableDescription { kind, name }) = callable_description {
                        format!(" of {kind} `{name}`")
                    } else {
                        String::new()
                    }
                ));
                // TODO: This should probably be adapted to handle more
                // types of callables[1]. At present, it just handles
                // standard function and method calls.
                //
                // [1]: https://github.com/astral-sh/ty/issues/274#issuecomment-2881856028
                let function_type_and_kind = match self.signature_type {
                    Type::FunctionLiteral(function) => Some(("function", function)),
                    Type::BoundMethod(bound_method) => {
                        Some(("bound method", bound_method.function(context.db())))
                    }
                    _ => None,
                };
                if let Some((kind, function)) = function_type_and_kind {
                    if let Some(overloaded_function) = function.to_overloaded(context.db()) {
                        if let Some(spans) = overloaded_function
                            .overloads
                            .first()
                            .and_then(|overload| overload.spans(context.db()))
                        {
                            let mut sub =
                                SubDiagnostic::new(Severity::Info, "First overload defined here");
                            sub.annotate(Annotation::primary(spans.signature));
                            diag.sub(sub);
                        }

                        diag.info(format_args!(
                            "Possible overloads for {kind} `{}`:",
                            function.name(context.db())
                        ));

                        let overloads = &function.signature(context.db()).overloads.overloads;
                        for overload in overloads.iter().take(MAXIMUM_OVERLOADS) {
                            diag.info(format_args!("  {}", overload.display(context.db())));
                        }
                        if overloads.len() > MAXIMUM_OVERLOADS {
                            diag.info(format_args!(
                                "... omitted {remaining} overloads",
                                remaining = overloads.len() - MAXIMUM_OVERLOADS
                            ));
                        }

                        if let Some(spans) = overloaded_function
                            .implementation
                            .and_then(|function| function.spans(context.db()))
                        {
                            let mut sub = SubDiagnostic::new(
                                Severity::Info,
                                "Overload implementation defined here",
                            );
                            sub.annotate(Annotation::primary(spans.signature));
                            diag.sub(sub);
                        }
                    }
                }
                if let Some(union_diag) = union_diag {
                    union_diag.add_union_context(context.db(), &mut diag);
                }
            }
        }
    }
}

/// Binding information for one of the overloads of a callable.
#[derive(Debug)]
pub(crate) struct Binding<'db> {
    /// Return type of the call.
    return_ty: Type<'db>,

    /// The specialization that was inferred from the argument types, if the callable is generic.
    specialization: Option<Specialization<'db>>,

    /// The specialization that was inferred for a class method's containing generic class, if it
    /// is being used to infer a specialization for the class.
    inherited_specialization: Option<Specialization<'db>>,

    /// The formal parameter that each argument is matched with, in argument source order, or
    /// `None` if the argument was not matched to any parameter.
    argument_parameters: Box<[Option<usize>]>,

    /// Bound types for parameters, in parameter source order, or `None` if no argument was matched
    /// to that parameter.
    parameter_tys: Box<[Option<Type<'db>>]>,

    /// Call binding errors, if any.
    errors: Vec<BindingError<'db>>,
}

impl<'db> Binding<'db> {
    fn match_parameters(
        signature: &Signature<'db>,
        arguments: &CallArguments<'_>,
        argument_forms: &mut [Option<ParameterForm>],
        conflicting_forms: &mut [bool],
    ) -> Self {
        let parameters = signature.parameters();
        // The parameter that each argument is matched with.
        let mut argument_parameters = vec![None; arguments.len()];
        // Whether each parameter has been matched with an argument.
        let mut parameter_matched = vec![false; parameters.len()];
        let mut errors = vec![];
        let mut next_positional = 0;
        let mut first_excess_positional = None;
        let mut num_synthetic_args = 0;
        let get_argument_index = |argument_index: usize, num_synthetic_args: usize| {
            if argument_index >= num_synthetic_args {
                // Adjust the argument index to skip synthetic args, which don't appear at the call
                // site and thus won't be in the Call node arguments list.
                Some(argument_index - num_synthetic_args)
            } else {
                // we are erroring on a synthetic argument, we'll just emit the diagnostic on the
                // entire Call node, since there's no argument node for this argument at the call site
                None
            }
        };
        for (argument_index, argument) in arguments.iter().enumerate() {
            let (index, parameter, positional) = match argument {
                Argument::Positional | Argument::Synthetic => {
                    if matches!(argument, Argument::Synthetic) {
                        num_synthetic_args += 1;
                    }
                    let Some((index, parameter)) = parameters
                        .get_positional(next_positional)
                        .map(|param| (next_positional, param))
                        .or_else(|| parameters.variadic())
                    else {
                        first_excess_positional.get_or_insert(argument_index);
                        next_positional += 1;
                        continue;
                    };
                    next_positional += 1;
                    (index, parameter, !parameter.is_variadic())
                }
                Argument::Keyword(name) => {
                    let Some((index, parameter)) = parameters
                        .keyword_by_name(name)
                        .or_else(|| parameters.keyword_variadic())
                    else {
                        errors.push(BindingError::UnknownArgument {
                            argument_name: ast::name::Name::new(name),
                            argument_index: get_argument_index(argument_index, num_synthetic_args),
                        });
                        continue;
                    };
                    (index, parameter, false)
                }

                Argument::Variadic | Argument::Keywords => {
                    // TODO
                    continue;
                }
            };
            if !matches!(argument, Argument::Synthetic) {
                if let Some(existing) =
                    argument_forms[argument_index - num_synthetic_args].replace(parameter.form)
                {
                    if existing != parameter.form {
                        conflicting_forms[argument_index - num_synthetic_args] = true;
                    }
                }
            }
            if parameter_matched[index] {
                if !parameter.is_variadic() && !parameter.is_keyword_variadic() {
                    errors.push(BindingError::ParameterAlreadyAssigned {
                        argument_index: get_argument_index(argument_index, num_synthetic_args),
                        parameter: ParameterContext::new(parameter, index, positional),
                    });
                }
            }
            argument_parameters[argument_index] = Some(index);
            parameter_matched[index] = true;
        }
        if let Some(first_excess_argument_index) = first_excess_positional {
            errors.push(BindingError::TooManyPositionalArguments {
                first_excess_argument_index: get_argument_index(
                    first_excess_argument_index,
                    num_synthetic_args,
                ),
                expected_positional_count: parameters.positional().count(),
                provided_positional_count: next_positional,
            });
        }
        let mut missing = vec![];
        for (index, matched) in parameter_matched.iter().copied().enumerate() {
            if !matched {
                let param = &parameters[index];
                if param.is_variadic()
                    || param.is_keyword_variadic()
                    || param.default_type().is_some()
                {
                    // variadic/keywords and defaulted arguments are not required
                    continue;
                }
                missing.push(ParameterContext::new(param, index, false));
            }
        }

        if !missing.is_empty() {
            errors.push(BindingError::MissingArguments {
                parameters: ParameterContexts(missing),
            });
        }

        Self {
            return_ty: signature.return_ty.unwrap_or(Type::unknown()),
            specialization: None,
            inherited_specialization: None,
            argument_parameters: argument_parameters.into_boxed_slice(),
            parameter_tys: vec![None; parameters.len()].into_boxed_slice(),
            errors,
        }
    }

    fn check_types(
        &mut self,
        db: &'db dyn Db,
        signature: &Signature<'db>,
        argument_types: &CallArgumentTypes<'_, 'db>,
    ) {
        let mut num_synthetic_args = 0;
        let get_argument_index = |argument_index: usize, num_synthetic_args: usize| {
            if argument_index >= num_synthetic_args {
                // Adjust the argument index to skip synthetic args, which don't appear at the call
                // site and thus won't be in the Call node arguments list.
                Some(argument_index - num_synthetic_args)
            } else {
                // we are erroring on a synthetic argument, we'll just emit the diagnostic on the
                // entire Call node, since there's no argument node for this argument at the call site
                None
            }
        };

        // If this overload is generic, first see if we can infer a specialization of the function
        // from the arguments that were passed in.
        let parameters = signature.parameters();
        if signature.generic_context.is_some() || signature.inherited_generic_context.is_some() {
            let mut builder = SpecializationBuilder::new(db);
            for (argument_index, (argument, argument_type)) in argument_types.iter().enumerate() {
                if matches!(argument, Argument::Synthetic) {
                    num_synthetic_args += 1;
                }
                let Some(parameter_index) = self.argument_parameters[argument_index] else {
                    // There was an error with argument when matching parameters, so don't bother
                    // type-checking it.
                    continue;
                };
                let parameter = &parameters[parameter_index];
                let Some(expected_type) = parameter.annotated_type() else {
                    continue;
                };
                if let Err(error) = builder.infer(expected_type, argument_type) {
                    self.errors.push(BindingError::SpecializationError {
                        error,
                        argument_index: get_argument_index(argument_index, num_synthetic_args),
                    });
                }
            }
            self.specialization = signature.generic_context.map(|gc| builder.build(gc));
            self.inherited_specialization = signature.inherited_generic_context.map(|gc| {
                // The inherited generic context is used when inferring the specialization of a
                // generic class from a constructor call. In this case (only), we promote any
                // typevars that are inferred as a literal to the corresponding instance type.
                builder
                    .build(gc)
                    .apply_type_mapping(db, &TypeMapping::PromoteLiterals)
            });
        }

        num_synthetic_args = 0;
        for (argument_index, (argument, mut argument_type)) in argument_types.iter().enumerate() {
            if matches!(argument, Argument::Synthetic) {
                num_synthetic_args += 1;
            }
            let Some(parameter_index) = self.argument_parameters[argument_index] else {
                // There was an error with argument when matching parameters, so don't bother
                // type-checking it.
                continue;
            };
            let parameter = &parameters[parameter_index];
            if let Some(mut expected_ty) = parameter.annotated_type() {
                if let Some(specialization) = self.specialization {
                    argument_type = argument_type.apply_specialization(db, specialization);
                    expected_ty = expected_ty.apply_specialization(db, specialization);
                }
                if let Some(inherited_specialization) = self.inherited_specialization {
                    argument_type =
                        argument_type.apply_specialization(db, inherited_specialization);
                    expected_ty = expected_ty.apply_specialization(db, inherited_specialization);
                }
                if !argument_type.is_assignable_to(db, expected_ty) {
                    let positional = matches!(argument, Argument::Positional | Argument::Synthetic)
                        && !parameter.is_variadic();
                    self.errors.push(BindingError::InvalidArgumentType {
                        parameter: ParameterContext::new(parameter, parameter_index, positional),
                        argument_index: get_argument_index(argument_index, num_synthetic_args),
                        expected_ty,
                        provided_ty: argument_type,
                    });
                }
            }
            // We still update the actual type of the parameter in this binding to match the
            // argument, even if the argument type is not assignable to the expected parameter
            // type.
            if let Some(existing) = self.parameter_tys[parameter_index].replace(argument_type) {
                // We already verified in `match_parameters` that we only match multiple arguments
                // with variadic parameters.
                let union = UnionType::from_elements(db, [existing, argument_type]);
                self.parameter_tys[parameter_index] = Some(union);
            }
        }

        if let Some(specialization) = self.specialization {
            self.return_ty = self.return_ty.apply_specialization(db, specialization);
        }
        if let Some(inherited_specialization) = self.inherited_specialization {
            self.return_ty = self
                .return_ty
                .apply_specialization(db, inherited_specialization);
        }
    }

    pub(crate) fn set_return_type(&mut self, return_ty: Type<'db>) {
        self.return_ty = return_ty;
    }

    pub(crate) fn return_type(&self) -> Type<'db> {
        self.return_ty
    }

    pub(crate) fn inherited_specialization(&self) -> Option<Specialization<'db>> {
        self.inherited_specialization
    }

    /// Returns the bound types for each parameter, in parameter source order, or `None` if no
    /// argument was matched to that parameter.
    pub(crate) fn parameter_types(&self) -> &[Option<Type<'db>>] {
        &self.parameter_tys
    }

    pub(crate) fn arguments_for_parameter<'a>(
        &'a self,
        argument_types: &'a CallArgumentTypes<'a, 'db>,
        parameter_index: usize,
    ) -> impl Iterator<Item = (Argument<'a>, Type<'db>)> + 'a {
        argument_types
            .iter()
            .zip(&self.argument_parameters)
            .filter(move |(_, argument_parameter)| {
                argument_parameter.is_some_and(|ap| ap == parameter_index)
            })
            .map(|(arg_and_type, _)| arg_and_type)
    }

    fn report_diagnostics(
        &self,
        context: &InferContext<'db>,
        node: ast::AnyNodeRef,
        callable_ty: Type<'db>,
        callable_description: Option<&CallableDescription>,
        union_diag: Option<&UnionDiagnostic<'_, '_>>,
    ) {
        for error in &self.errors {
            error.report_diagnostic(context, node, callable_ty, callable_description, union_diag);
        }
    }

    fn as_result(&self) -> Result<(), CallErrorKind> {
        if !self.errors.is_empty() {
            return Err(CallErrorKind::BindingError);
        }
        Ok(())
    }
}

/// Describes a callable for the purposes of diagnostics.
#[derive(Debug)]
pub(crate) struct CallableDescription<'a> {
    name: &'a str,
    kind: &'a str,
}

impl<'db> CallableDescription<'db> {
    fn new(db: &'db dyn Db, callable_type: Type<'db>) -> Option<CallableDescription<'db>> {
        match callable_type {
            Type::FunctionLiteral(function) => Some(CallableDescription {
                kind: "function",
                name: function.name(db),
            }),
            Type::ClassLiteral(class_type) => Some(CallableDescription {
                kind: "class",
                name: class_type.name(db),
            }),
            Type::BoundMethod(bound_method) => Some(CallableDescription {
                kind: "bound method",
                name: bound_method.function(db).name(db),
            }),
            Type::MethodWrapper(MethodWrapperKind::FunctionTypeDunderGet(function)) => {
                Some(CallableDescription {
                    kind: "method wrapper `__get__` of function",
                    name: function.name(db),
                })
            }
            Type::MethodWrapper(MethodWrapperKind::PropertyDunderGet(_)) => {
                Some(CallableDescription {
                    kind: "method wrapper",
                    name: "`__get__` of property",
                })
            }
            Type::WrapperDescriptor(kind) => Some(CallableDescription {
                kind: "wrapper descriptor",
                name: match kind {
                    WrapperDescriptorKind::FunctionTypeDunderGet => "FunctionType.__get__",
                    WrapperDescriptorKind::PropertyDunderGet => "property.__get__",
                    WrapperDescriptorKind::PropertyDunderSet => "property.__set__",
                },
            }),
            _ => None,
        }
    }
}

/// Information needed to emit a diagnostic regarding a parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParameterContext {
    name: Option<ast::name::Name>,
    index: usize,

    /// Was the argument for this parameter passed positionally, and matched to a non-variadic
    /// positional parameter? (If so, we will provide the index in the diagnostic, not just the
    /// name.)
    positional: bool,
}

impl ParameterContext {
    fn new(parameter: &Parameter, index: usize, positional: bool) -> Self {
        Self {
            name: parameter.display_name(),
            index,
            positional,
        }
    }
}

impl std::fmt::Display for ParameterContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(name) = &self.name {
            if self.positional {
                write!(f, "{} (`{name}`)", self.index + 1)
            } else {
                write!(f, "`{name}`")
            }
        } else {
            write!(f, "{}", self.index + 1)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParameterContexts(Vec<ParameterContext>);

impl std::fmt::Display for ParameterContexts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut iter = self.0.iter();
        if let Some(first) = iter.next() {
            write!(f, "{first}")?;
            for param in iter {
                f.write_str(", ")?;
                write!(f, "{param}")?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BindingError<'db> {
    /// The type of an argument is not assignable to the annotated type of its corresponding
    /// parameter.
    InvalidArgumentType {
        parameter: ParameterContext,
        argument_index: Option<usize>,
        expected_ty: Type<'db>,
        provided_ty: Type<'db>,
    },
    /// One or more required parameters (that is, with no default) is not supplied by any argument.
    MissingArguments { parameters: ParameterContexts },
    /// A call argument can't be matched to any parameter.
    UnknownArgument {
        argument_name: ast::name::Name,
        argument_index: Option<usize>,
    },
    /// More positional arguments are provided in the call than can be handled by the signature.
    TooManyPositionalArguments {
        first_excess_argument_index: Option<usize>,
        expected_positional_count: usize,
        provided_positional_count: usize,
    },
    /// Multiple arguments were provided for a single parameter.
    ParameterAlreadyAssigned {
        argument_index: Option<usize>,
        parameter: ParameterContext,
    },
    /// An inferred specialization was invalid.
    SpecializationError {
        error: SpecializationError<'db>,
        argument_index: Option<usize>,
    },
    /// The call itself might be well constructed, but an error occurred while evaluating the call.
    /// We use this variant to report errors in `property.__get__` and `property.__set__`, which
    /// can occur when the call to the underlying getter/setter fails.
    InternalCallError(&'static str),
}

impl<'db> BindingError<'db> {
    fn report_diagnostic(
        &self,
        context: &InferContext<'db>,
        node: ast::AnyNodeRef,
        callable_ty: Type<'db>,
        callable_description: Option<&CallableDescription>,
        union_diag: Option<&UnionDiagnostic<'_, '_>>,
    ) {
        match self {
            Self::InvalidArgumentType {
                parameter,
                argument_index,
                expected_ty,
                provided_ty,
            } => {
                let range = Self::get_node(node, *argument_index);
                let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, range) else {
                    return;
                };

                let provided_ty_display = provided_ty.display(context.db());
                let expected_ty_display = expected_ty.display(context.db());

                let mut diag = builder.into_diagnostic(format_args!(
                    "Argument{} is incorrect",
                    if let Some(CallableDescription { kind, name }) = callable_description {
                        format!(" to {kind} `{name}`")
                    } else {
                        String::new()
                    }
                ));
                diag.set_primary_message(format_args!(
                    "Expected `{expected_ty_display}`, found `{provided_ty_display}`"
                ));
                if let Some((name_span, parameter_span)) =
                    callable_ty.parameter_span(context.db(), Some(parameter.index))
                {
                    let mut sub = SubDiagnostic::new(Severity::Info, "Function defined here");
                    sub.annotate(Annotation::primary(name_span));
                    sub.annotate(
                        Annotation::secondary(parameter_span).message("Parameter declared here"),
                    );
                    diag.sub(sub);
                }
                if let Some(union_diag) = union_diag {
                    union_diag.add_union_context(context.db(), &mut diag);
                }
            }

            Self::TooManyPositionalArguments {
                first_excess_argument_index,
                expected_positional_count,
                provided_positional_count,
            } => {
                let node = Self::get_node(node, *first_excess_argument_index);
                if let Some(builder) = context.report_lint(&TOO_MANY_POSITIONAL_ARGUMENTS, node) {
                    let mut diag = builder.into_diagnostic(format_args!(
                        "Too many positional arguments{}: expected \
                        {expected_positional_count}, got {provided_positional_count}",
                        if let Some(CallableDescription { kind, name }) = callable_description {
                            format!(" to {kind} `{name}`")
                        } else {
                            String::new()
                        }
                    ));
                    if let Some(union_diag) = union_diag {
                        union_diag.add_union_context(context.db(), &mut diag);
                    }
                }
            }

            Self::MissingArguments { parameters } => {
                if let Some(builder) = context.report_lint(&MISSING_ARGUMENT, node) {
                    let s = if parameters.0.len() == 1 { "" } else { "s" };
                    let mut diag = builder.into_diagnostic(format_args!(
                        "No argument{s} provided for required parameter{s} {parameters}{}",
                        if let Some(CallableDescription { kind, name }) = callable_description {
                            format!(" of {kind} `{name}`")
                        } else {
                            String::new()
                        }
                    ));
                    if let Some(union_diag) = union_diag {
                        union_diag.add_union_context(context.db(), &mut diag);
                    }
                }
            }

            Self::UnknownArgument {
                argument_name,
                argument_index,
            } => {
                let node = Self::get_node(node, *argument_index);
                if let Some(builder) = context.report_lint(&UNKNOWN_ARGUMENT, node) {
                    let mut diag = builder.into_diagnostic(format_args!(
                        "Argument `{argument_name}` does not match any known parameter{}",
                        if let Some(CallableDescription { kind, name }) = callable_description {
                            format!(" of {kind} `{name}`")
                        } else {
                            String::new()
                        }
                    ));
                    if let Some(union_diag) = union_diag {
                        union_diag.add_union_context(context.db(), &mut diag);
                    }
                }
            }

            Self::ParameterAlreadyAssigned {
                argument_index,
                parameter,
            } => {
                let node = Self::get_node(node, *argument_index);
                if let Some(builder) = context.report_lint(&PARAMETER_ALREADY_ASSIGNED, node) {
                    let mut diag = builder.into_diagnostic(format_args!(
                        "Multiple values provided for parameter {parameter}{}",
                        if let Some(CallableDescription { kind, name }) = callable_description {
                            format!(" of {kind} `{name}`")
                        } else {
                            String::new()
                        }
                    ));
                    if let Some(union_diag) = union_diag {
                        union_diag.add_union_context(context.db(), &mut diag);
                    }
                }
            }

            Self::SpecializationError {
                error,
                argument_index,
            } => {
                let range = Self::get_node(node, *argument_index);
                let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, range) else {
                    return;
                };

                let typevar = error.typevar();
                let argument_type = error.argument_type();
                let argument_ty_display = argument_type.display(context.db());

                let mut diag = builder.into_diagnostic(format_args!(
                    "Argument{} is incorrect",
                    if let Some(CallableDescription { kind, name }) = callable_description {
                        format!(" to {kind} `{name}`")
                    } else {
                        String::new()
                    }
                ));
                diag.set_primary_message(format_args!(
                    "Argument type `{argument_ty_display}` does not satisfy {} of type variable `{}`",
                    match error {
                        SpecializationError::MismatchedBound {..} => "upper bound",
                        SpecializationError::MismatchedConstraint {..} => "constraints",
                    },
                    typevar.name(context.db()),
                ));

                if let Some(typevar_definition) = typevar.definition(context.db()) {
                    let typevar_range = typevar_definition.full_range(context.db());
                    let mut sub = SubDiagnostic::new(Severity::Info, "Type variable defined here");
                    sub.annotate(Annotation::primary(typevar_range.into()));
                    diag.sub(sub);
                }

                if let Some(union_diag) = union_diag {
                    union_diag.add_union_context(context.db(), &mut diag);
                }
            }

            Self::InternalCallError(reason) => {
                let node = Self::get_node(node, None);
                if let Some(builder) = context.report_lint(&CALL_NON_CALLABLE, node) {
                    let mut diag = builder.into_diagnostic(format_args!(
                        "Call{} failed: {reason}",
                        if let Some(CallableDescription { kind, name }) = callable_description {
                            format!(" of {kind} `{name}`")
                        } else {
                            String::new()
                        }
                    ));
                    if let Some(union_diag) = union_diag {
                        union_diag.add_union_context(context.db(), &mut diag);
                    }
                }
            }
        }
    }

    fn get_node(node: ast::AnyNodeRef, argument_index: Option<usize>) -> ast::AnyNodeRef {
        // If we have a Call node and an argument index, report the diagnostic on the correct
        // argument node; otherwise, report it on the entire provided node.
        match (node, argument_index) {
            (ast::AnyNodeRef::ExprCall(call_node), Some(argument_index)) => {
                match call_node
                    .arguments
                    .arguments_source_order()
                    .nth(argument_index)
                    .expect("argument index should not be out of range")
                {
                    ast::ArgOrKeyword::Arg(expr) => expr.into(),
                    ast::ArgOrKeyword::Keyword(keyword) => keyword.into(),
                }
            }
            _ => node,
        }
    }
}

/// Contains additional context for union specific diagnostics.
///
/// This is used when a function call is inconsistent with one or more variants
/// of a union. This can be used to attach sub-diagnostics that clarify that
/// the error is part of a union.
struct UnionDiagnostic<'b, 'db> {
    /// The type of the union.
    callable_type: Type<'db>,
    /// The specific binding that failed.
    binding: &'b CallableBinding<'db>,
}

impl UnionDiagnostic<'_, '_> {
    /// Adds context about any relevant union function types to the given
    /// diagnostic.
    fn add_union_context(&self, db: &'_ dyn Db, diag: &mut Diagnostic) {
        let sub = SubDiagnostic::new(
            Severity::Info,
            format_args!(
                "Union variant `{callable_ty}` is incompatible with this call site",
                callable_ty = self.binding.callable_type.display(db),
            ),
        );
        diag.sub(sub);

        let sub = SubDiagnostic::new(
            Severity::Info,
            format_args!(
                "Attempted to call union type `{}`",
                self.callable_type.display(db)
            ),
        );
        diag.sub(sub);
    }
}
