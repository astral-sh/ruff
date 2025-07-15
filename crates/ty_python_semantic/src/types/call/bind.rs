//! When analyzing a call site, we create _bindings_, which match and type-check the actual
//! arguments against the parameters of the callable. Like with
//! [signatures][crate::types::signatures], we have to handle the fact that the callable might be a
//! union of types, each of which might contain multiple overloads.

use std::collections::HashSet;
use std::fmt;

use itertools::Itertools;
use ruff_db::parsed::parsed_module;
use smallvec::{SmallVec, smallvec};

use super::{Argument, CallArguments, CallError, CallErrorKind, InferContext, Signature, Type};
use crate::db::Db;
use crate::dunder_all::dunder_all_names;
use crate::place::{Boundness, Place};
use crate::types::diagnostic::{
    CALL_NON_CALLABLE, CONFLICTING_ARGUMENT_FORMS, INVALID_ARGUMENT_TYPE, MISSING_ARGUMENT,
    NO_MATCHING_OVERLOAD, PARAMETER_ALREADY_ASSIGNED, TOO_MANY_POSITIONAL_ARGUMENTS,
    UNKNOWN_ARGUMENT,
};
use crate::types::function::{
    DataclassTransformerParams, FunctionDecorators, FunctionType, KnownFunction, OverloadLiteral,
};
use crate::types::generics::{Specialization, SpecializationBuilder, SpecializationError};
use crate::types::signatures::{Parameter, ParameterForm, Parameters};
use crate::types::tuple::TupleType;
use crate::types::{
    BoundMethodType, ClassLiteral, DataclassParams, KnownClass, KnownInstanceType,
    MethodWrapperKind, PropertyInstanceType, SpecialFormType, TypeMapping, UnionType,
    WrapperDescriptorKind, enums, ide_support, todo_type,
};
use ruff_db::diagnostic::{Annotation, Diagnostic, Severity, SubDiagnostic};
use ruff_python_ast as ast;

/// Binding information for a possible union of callables. At a call site, the arguments must be
/// compatible with _all_ of the types in the union for the call to be valid.
///
/// It's guaranteed that the wrapped bindings have no errors.
#[derive(Debug)]
pub(crate) struct Bindings<'db> {
    /// The type that is (hopefully) callable.
    callable_type: Type<'db>,

    /// By using `SmallVec`, we avoid an extra heap allocation for the common case of a non-union
    /// type.
    elements: SmallVec<[CallableBinding<'db>; 1]>,

    /// Whether each argument will be used as a value and/or a type form in this call.
    pub(crate) argument_forms: Box<[Option<ParameterForm>]>,

    conflicting_forms: Box<[bool]>,
}

impl<'db> Bindings<'db> {
    /// Creates a new `Bindings` from an iterator of [`Bindings`]s. Panics if the iterator is
    /// empty.
    pub(crate) fn from_union<I>(callable_type: Type<'db>, elements: I) -> Self
    where
        I: IntoIterator<Item = Bindings<'db>>,
    {
        let elements: SmallVec<_> = elements
            .into_iter()
            .flat_map(|s| s.elements.into_iter())
            .collect();
        assert!(!elements.is_empty());
        Self {
            callable_type,
            elements,
            argument_forms: Box::from([]),
            conflicting_forms: Box::from([]),
        }
    }

    pub(crate) fn replace_callable_type(&mut self, before: Type<'db>, after: Type<'db>) {
        if self.callable_type == before {
            self.callable_type = after;
        }
        for binding in &mut self.elements {
            binding.replace_callable_type(before, after);
        }
    }

    pub(crate) fn set_dunder_call_is_possibly_unbound(&mut self) {
        for binding in &mut self.elements {
            binding.dunder_call_is_possibly_unbound = true;
        }
    }

    /// Match the arguments of a call site against the parameters of a collection of possibly
    /// unioned, possibly overloaded signatures.
    ///
    /// The returned bindings tell you which parameter (in each signature) each argument was
    /// matched against. You can then perform type inference on each argument with extra context
    /// about the expected parameter types.
    ///
    /// Once you have argument types available, you can call [`check_types`][Self::check_types] to
    /// verify that each argument type is assignable to the corresponding parameter type.
    pub(crate) fn match_parameters(mut self, arguments: &CallArguments<'_, 'db>) -> Self {
        let mut argument_forms = vec![None; arguments.len()];
        let mut conflicting_forms = vec![false; arguments.len()];
        for binding in &mut self.elements {
            binding.match_parameters(arguments, &mut argument_forms, &mut conflicting_forms);
        }
        self.argument_forms = argument_forms.into();
        self.conflicting_forms = conflicting_forms.into();
        self
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
        argument_types: &CallArguments<'_, 'db>,
    ) -> Result<Self, CallError<'db>> {
        for element in &mut self.elements {
            element.check_types(db, argument_types);
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
        self.callable_type
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
    pub(crate) fn report_diagnostics(
        &self,
        context: &InferContext<'db, '_>,
        node: ast::AnyNodeRef,
    ) {
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

        // Each special case listed here should have a corresponding clause in `Type::bindings`.
        for binding in &mut self.elements {
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
                        } else if function.has_known_decorator(db, FunctionDecorators::STATICMETHOD)
                        {
                            overload.set_return_type(Type::FunctionLiteral(function));
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
                            } else if function
                                .has_known_decorator(db, FunctionDecorators::STATICMETHOD)
                            {
                                overload.set_return_type(*function_ty);
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
                                            typevar.constraints(db).into_iter().flatten().copied(),
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
                                        .try_call(db, &CallArguments::positional([*instance]))
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
                                        .try_call(db, &CallArguments::positional([*instance]))
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
                                if let Err(_call_error) = setter
                                    .try_call(db, &CallArguments::positional([*instance, *value]))
                                {
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
                                if let Err(_call_error) = setter
                                    .try_call(db, &CallArguments::positional([*instance, *value]))
                                {
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

                        Some(KnownFunction::EnumMembers) => {
                            if let [Some(ty)] = overload.parameter_types() {
                                let return_ty = match ty {
                                    Type::ClassLiteral(class) => {
                                        if let Some(metadata) = enums::enum_metadata(db, *class) {
                                            TupleType::from_elements(
                                                db,
                                                metadata
                                                    .members
                                                    .iter()
                                                    .map(|member| Type::string_literal(db, member)),
                                            )
                                        } else {
                                            Type::unknown()
                                        }
                                    }
                                    _ => Type::unknown(),
                                };

                                overload.set_return_type(return_ty);
                            }
                        }

                        Some(KnownFunction::AllMembers) => {
                            if let [Some(ty)] = overload.parameter_types() {
                                overload.set_return_type(TupleType::from_elements(
                                    db,
                                    ide_support::all_members(db, *ty)
                                        .into_iter()
                                        .sorted()
                                        .map(|member| Type::string_literal(db, &member.name)),
                                ));
                            }
                        }

                        Some(KnownFunction::TopMaterialization) => {
                            if let [Some(ty)] = overload.parameter_types() {
                                overload.set_return_type(ty.top_materialization(db));
                            }
                        }

                        Some(KnownFunction::BottomMaterialization) => {
                            if let [Some(ty)] = overload.parameter_types() {
                                overload.set_return_type(ty.bottom_materialization(db));
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
                                    Place::Type(ty, Boundness::Bound) => {
                                        if ty.is_dynamic() {
                                            // Here, we attempt to model the fact that an attribute lookup on
                                            // a dynamic type could fail

                                            union_with_default(ty)
                                        } else {
                                            ty
                                        }
                                    }
                                    Place::Type(ty, Boundness::PossiblyUnbound) => {
                                        union_with_default(ty)
                                    }
                                    Place::Unbound => default,
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

                            // `dataclass` being used as a non-decorator
                            if let [Some(Type::ClassLiteral(class_literal))] =
                                overload.parameter_types()
                            {
                                let params = DataclassParams::default();
                                overload.set_return_type(Type::from(ClassLiteral::new(
                                    db,
                                    class_literal.name(db),
                                    class_literal.body_scope(db),
                                    class_literal.known(db),
                                    Some(params),
                                    class_literal.dataclass_transformer_params(db),
                                )));
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
                            // Ideally, either the implementation, or exactly one of the overloads
                            // of the function can have the dataclass_transform decorator applied.
                            // However, we do not yet enforce this, and in the case of multiple
                            // applications of the decorator, we will only consider the last one
                            // for the return value, since the prior ones will be over-written.
                            let return_type = function_type
                                .iter_overloads_and_implementation(db)
                                .filter_map(|function_overload| {
                                    function_overload.dataclass_transformer_params(db).map(
                                        |params| {
                                            // This is a call to a custom function that was decorated with `@dataclass_transformer`.
                                            // If this function was called with a keyword argument like `order=False`, we extract
                                            // the argument type and overwrite the corresponding flag in `dataclass_params` after
                                            // constructing them from the `dataclass_transformer`-parameter defaults.

                                            let mut dataclass_params =
                                                DataclassParams::from(params);

                                            if let Some(Some(Type::BooleanLiteral(order))) =
                                                overload
                                                    .signature
                                                    .parameters()
                                                    .keyword_by_name("order")
                                                    .map(|(idx, _)| idx)
                                                    .and_then(|idx| {
                                                        overload.parameter_types().get(idx)
                                                    })
                                            {
                                                dataclass_params
                                                    .set(DataclassParams::ORDER, *order);
                                            }

                                            Type::DataclassDecorator(dataclass_params)
                                        },
                                    )
                                })
                                .last();

                            if let Some(return_type) = return_type {
                                overload.set_return_type(return_type);
                            }
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

                        Some(KnownClass::Tuple) if overload_index == 1 => {
                            // `tuple(range(42))` => `tuple[int, ...]`
                            // BUT `tuple((1, 2))` => `tuple[Literal[1], Literal[2]]` rather than `tuple[Literal[1, 2], ...]`
                            if let [Some(argument)] = overload.parameter_types() {
                                let overridden_return =
                                    argument.into_tuple().map(Type::Tuple).unwrap_or_else(|| {
                                        // Some awkward special handling is required here because of the fact
                                        // that calling `try_iterate()` on `Never` returns `Never`,
                                        // but `tuple[Never, ...]` eagerly simplifies to `tuple[()]`,
                                        // which will cause us to emit false positives if we index into the tuple.
                                        // Using `tuple[Unknown, ...]` avoids these false positives.
                                        let specialization = if argument.is_never() {
                                            Type::unknown()
                                        } else {
                                            argument.try_iterate(db).expect(
                                                "try_iterate() should not fail on a type \
                                                    assignable to `Iterable`",
                                            )
                                        };
                                        TupleType::homogeneous(db, specialization)
                                    });
                                overload.set_return_type(overridden_return);
                            }
                        }

                        _ => {}
                    },

                    Type::SpecialForm(SpecialFormType::TypedDict) => {
                        overload.set_return_type(todo_type!("Support for `TypedDict`"));
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

impl<'db> From<CallableBinding<'db>> for Bindings<'db> {
    fn from(from: CallableBinding<'db>) -> Bindings<'db> {
        Bindings {
            callable_type: from.callable_type,
            elements: smallvec![from],
            argument_forms: Box::from([]),
            conflicting_forms: Box::from([]),
        }
    }
}

impl<'db> From<Binding<'db>> for Bindings<'db> {
    fn from(from: Binding<'db>) -> Bindings<'db> {
        let callable_type = from.callable_type;
        let signature_type = from.signature_type;
        let callable_binding = CallableBinding {
            callable_type,
            signature_type,
            dunder_call_is_possibly_unbound: false,
            bound_type: None,
            overload_call_return_type: None,
            matching_overload_index: None,
            overloads: smallvec![from],
        };
        Bindings {
            callable_type,
            elements: smallvec![callable_binding],
            argument_forms: Box::from([]),
            conflicting_forms: Box::from([]),
        }
    }
}

/// Binding information for a single callable. If the callable is overloaded, there is a separate
/// [`Binding`] for each overload.
///
/// For a successful binding, each argument is mapped to one of the callable's formal parameters.
/// If the callable has multiple overloads, the first one that matches is used as the overall
/// binding match.
///
/// If the arguments cannot be matched to formal parameters, we store information about the
/// specific errors that occurred when trying to match them up. If the callable has multiple
/// overloads, we store this error information for each overload.
#[derive(Debug)]
pub(crate) struct CallableBinding<'db> {
    /// The type that is (hopefully) callable.
    pub(crate) callable_type: Type<'db>,

    /// The type we'll use for error messages referring to details of the called signature. For
    /// calls to functions this will be the same as `callable_type`; for other callable instances
    /// it may be a `__call__` method.
    pub(crate) signature_type: Type<'db>,

    /// If this is a callable object (i.e. called via a `__call__` method), the boundness of
    /// that call method.
    pub(crate) dunder_call_is_possibly_unbound: bool,

    /// The type of the bound `self` or `cls` parameter if this signature is for a bound method.
    pub(crate) bound_type: Option<Type<'db>>,

    /// The return type of this overloaded callable.
    ///
    /// This is [`Some`] only in the following cases:
    /// 1. Argument type expansion was performed and one of the expansions evaluated successfully
    ///    for all of the argument lists, or
    /// 2. Overload call evaluation was ambiguous, meaning that multiple overloads matched the
    ///    argument lists, but they all had different return types
    ///
    /// For (1), the final return type is the union of all the return types of the matched
    /// overloads for the expanded argument lists.
    ///
    /// For (2), the final return type is [`Unknown`].
    ///
    /// [`Unknown`]: crate::types::DynamicType::Unknown
    overload_call_return_type: Option<OverloadCallReturnType<'db>>,

    /// The index of the overload that matched for this overloaded callable.
    ///
    /// This is [`Some`] only for step 1 and 4 of the [overload call evaluation algorithm][1].
    ///
    /// The main use of this field is to surface the diagnostics for a matching overload directly
    /// instead of using the `no-matching-overload` diagnostic. This is mentioned in the spec:
    ///
    /// > If only one candidate overload remains, it is the winning match. Evaluate it as if it
    /// > were a non-overloaded function call and stop.
    ///
    /// Other steps of the algorithm do not set this field because this use case isn't relevant for
    /// them.
    ///
    /// [1]: https://typing.python.org/en/latest/spec/overload.html#overload-call-evaluation
    matching_overload_index: Option<usize>,

    /// The bindings of each overload of this callable. Will be empty if the type is not callable.
    ///
    /// By using `SmallVec`, we avoid an extra heap allocation for the common case of a
    /// non-overloaded callable.
    overloads: SmallVec<[Binding<'db>; 1]>,
}

impl<'db> CallableBinding<'db> {
    pub(crate) fn from_overloads(
        signature_type: Type<'db>,
        overloads: impl IntoIterator<Item = Signature<'db>>,
    ) -> Self {
        let overloads = overloads
            .into_iter()
            .map(|signature| Binding::single(signature_type, signature))
            .collect();
        Self {
            callable_type: signature_type,
            signature_type,
            dunder_call_is_possibly_unbound: false,
            bound_type: None,
            overload_call_return_type: None,
            matching_overload_index: None,
            overloads,
        }
    }

    pub(crate) fn not_callable(signature_type: Type<'db>) -> Self {
        Self {
            callable_type: signature_type,
            signature_type,
            dunder_call_is_possibly_unbound: false,
            bound_type: None,
            overload_call_return_type: None,
            matching_overload_index: None,
            overloads: smallvec![],
        }
    }

    pub(crate) fn with_bound_type(mut self, bound_type: Type<'db>) -> Self {
        self.bound_type = Some(bound_type);
        self
    }

    fn replace_callable_type(&mut self, before: Type<'db>, after: Type<'db>) {
        if self.callable_type == before {
            self.callable_type = after;
        }
        for binding in &mut self.overloads {
            binding.replace_callable_type(before, after);
        }
    }

    fn match_parameters(
        &mut self,
        arguments: &CallArguments<'_, 'db>,
        argument_forms: &mut [Option<ParameterForm>],
        conflicting_forms: &mut [bool],
    ) {
        // If this callable is a bound method, prepend the self instance onto the arguments list
        // before checking.
        let arguments = arguments.with_self(self.bound_type);

        for overload in &mut self.overloads {
            overload.match_parameters(arguments.as_ref(), argument_forms, conflicting_forms);
        }
    }

    fn check_types(&mut self, db: &'db dyn Db, argument_types: &CallArguments<'_, 'db>) {
        // If this callable is a bound method, prepend the self instance onto the arguments list
        // before checking.
        let argument_types = argument_types.with_self(self.bound_type);

        // Step 1: Check the result of the arity check which is done by `match_parameters`
        let matching_overload_indexes = match self.matching_overload_index() {
            MatchingOverloadIndex::None => {
                // If no candidate overloads remain from the arity check, we can stop here. We
                // still perform type checking for non-overloaded function to provide better user
                // experience.
                if let [overload] = self.overloads.as_mut_slice() {
                    overload.check_types(db, argument_types.as_ref());
                }
                return;
            }
            MatchingOverloadIndex::Single(index) => {
                // If only one candidate overload remains, it is the winning match. Evaluate it as
                // a regular (non-overloaded) call.
                self.matching_overload_index = Some(index);
                self.overloads[index].check_types(db, argument_types.as_ref());
                return;
            }
            MatchingOverloadIndex::Multiple(indexes) => {
                // If two or more candidate overloads remain, proceed to step 2.
                indexes
            }
        };

        let snapshotter = CallableBindingSnapshotter::new(matching_overload_indexes);

        // State of the bindings _before_ evaluating (type checking) the matching overloads using
        // the non-expanded argument types.
        let pre_evaluation_snapshot = snapshotter.take(self);

        // Step 2: Evaluate each remaining overload as a regular (non-overloaded) call to determine
        // whether it is compatible with the supplied argument list.
        for (_, overload) in self.matching_overloads_mut() {
            overload.check_types(db, argument_types.as_ref());
        }

        match self.matching_overload_index() {
            MatchingOverloadIndex::None => {
                // If all overloads result in errors, proceed to step 3.
            }
            MatchingOverloadIndex::Single(_) => {
                // If only one overload evaluates without error, it is the winning match.
                return;
            }
            MatchingOverloadIndex::Multiple(indexes) => {
                // If two or more candidate overloads remain, proceed to step 4.
                // TODO: Step 4

                // Step 5
                self.filter_overloads_using_any_or_unknown(db, argument_types.as_ref(), &indexes);

                // We're returning here because this shouldn't lead to argument type expansion.
                return;
            }
        }

        // Step 3: Perform "argument type expansion". Reference:
        // https://typing.python.org/en/latest/spec/overload.html#argument-type-expansion
        let mut expansions = argument_types.expand(db).peekable();

        if expansions.peek().is_none() {
            // Return early if there are no argument types to expand.
            return;
        }

        // State of the bindings _after_ evaluating (type checking) the matching overloads using
        // the non-expanded argument types.
        let post_evaluation_snapshot = snapshotter.take(self);

        // Restore the bindings state to the one prior to the type checking step in preparation
        // for evaluating the expanded argument lists.
        snapshotter.restore(self, pre_evaluation_snapshot);

        for expanded_argument_lists in expansions {
            // This is the merged state of the bindings after evaluating all of the expanded
            // argument lists. This will be the final state to restore the bindings to if all of
            // the expanded argument lists evaluated successfully.
            let mut merged_evaluation_state: Option<CallableBindingSnapshot<'db>> = None;

            let mut return_types = Vec::new();

            for expanded_argument_types in &expanded_argument_lists {
                let pre_evaluation_snapshot = snapshotter.take(self);

                for (_, overload) in self.matching_overloads_mut() {
                    overload.check_types(db, expanded_argument_types);
                }

                let return_type = match self.matching_overload_index() {
                    MatchingOverloadIndex::None => None,
                    MatchingOverloadIndex::Single(index) => {
                        Some(self.overloads[index].return_type())
                    }
                    MatchingOverloadIndex::Multiple(matching_overload_indexes) => {
                        // TODO: Step 4

                        self.filter_overloads_using_any_or_unknown(
                            db,
                            expanded_argument_types,
                            &matching_overload_indexes,
                        );

                        Some(self.return_type())
                    }
                };

                // This split between initializing and updating the merged evaluation state is
                // required because otherwise it's difficult to differentiate between the
                // following:
                // 1. An initial unmatched overload becomes a matched overload when evaluating the
                //    first argument list
                // 2. An unmatched overload after evaluating the first argument list becomes a
                //    matched overload when evaluating the second argument list
                if let Some(merged_evaluation_state) = merged_evaluation_state.as_mut() {
                    merged_evaluation_state.update(self);
                } else {
                    merged_evaluation_state = Some(snapshotter.take(self));
                }

                // Restore the bindings state before evaluating the next argument list.
                snapshotter.restore(self, pre_evaluation_snapshot);

                if let Some(return_type) = return_type {
                    return_types.push(return_type);
                } else {
                    // No need to check the remaining argument lists if the current argument list
                    // doesn't evaluate successfully. Move on to expanding the next argument type.
                    break;
                }
            }

            if return_types.len() == expanded_argument_lists.len() {
                // Restore the bindings state to the one that merges the bindings state evaluating
                // each of the expanded argument list.
                //
                // Note that this needs to happen *before* setting the return type, because this
                // will restore the return type to the one before argument type expansion.
                if let Some(merged_evaluation_state) = merged_evaluation_state {
                    snapshotter.restore(self, merged_evaluation_state);
                }

                // If the number of return types is equal to the number of expanded argument lists,
                // they all evaluated successfully. So, we need to combine their return types by
                // union to determine the final return type.
                self.overload_call_return_type =
                    Some(OverloadCallReturnType::ArgumentTypeExpansion(
                        UnionType::from_elements(db, return_types),
                    ));

                return;
            }
        }

        // If the type expansion didn't yield any successful return type, we need to restore the
        // bindings state back to the one after the type checking step using the non-expanded
        // argument types. This is necessary because we restore the state to the pre-evaluation
        // snapshot when processing the expanded argument lists.
        snapshotter.restore(self, post_evaluation_snapshot);
    }

    /// Filter overloads based on [`Any`] or [`Unknown`] argument types.
    ///
    /// This is the step 5 of the [overload call evaluation algorithm][1].
    ///
    /// The filtering works on the remaining overloads that are present at the
    /// `matching_overload_indexes` and are filtered out by marking them as unmatched overloads
    /// using the [`mark_as_unmatched_overload`] method.
    ///
    /// [`Any`]: crate::types::DynamicType::Any
    /// [`Unknown`]: crate::types::DynamicType::Unknown
    /// [`mark_as_unmatched_overload`]: Binding::mark_as_unmatched_overload
    /// [1]: https://typing.python.org/en/latest/spec/overload.html#overload-call-evaluation
    fn filter_overloads_using_any_or_unknown(
        &mut self,
        db: &'db dyn Db,
        arguments: &CallArguments<'_, 'db>,
        matching_overload_indexes: &[usize],
    ) {
        // These are the parameter indexes that matches the arguments that participate in the
        // filtering process.
        //
        // The parameter types at these indexes have at least one overload where the type isn't
        // gradual equivalent to the parameter types at the same index for other overloads.
        let mut participating_parameter_indexes = HashSet::new();

        // These only contain the top materialized argument types for the corresponding
        // participating parameter indexes.
        let mut top_materialized_argument_types = vec![];

        for (argument_index, argument_type) in arguments.iter_types().enumerate() {
            let mut first_parameter_type: Option<Type<'db>> = None;
            let mut participating_parameter_index = None;

            for overload_index in matching_overload_indexes {
                let overload = &self.overloads[*overload_index];
                let Some(parameter_index) = overload.argument_parameters[argument_index] else {
                    // There is no parameter for this argument in this overload.
                    break;
                };
                // TODO: For an unannotated `self` / `cls` parameter, the type should be
                // `typing.Self` / `type[typing.Self]`
                let current_parameter_type = overload.signature.parameters()[parameter_index]
                    .annotated_type()
                    .unwrap_or(Type::unknown());
                if let Some(first_parameter_type) = first_parameter_type {
                    if !first_parameter_type.is_equivalent_to(db, current_parameter_type) {
                        participating_parameter_index = Some(parameter_index);
                        break;
                    }
                } else {
                    first_parameter_type = Some(current_parameter_type);
                }
            }

            if let Some(parameter_index) = participating_parameter_index {
                participating_parameter_indexes.insert(parameter_index);
                top_materialized_argument_types.push(argument_type.top_materialization(db));
            }
        }

        let top_materialized_argument_type =
            TupleType::from_elements(db, top_materialized_argument_types);

        // A flag to indicate whether we've found the overload that makes the remaining overloads
        // unmatched for the given argument types.
        let mut filter_remaining_overloads = false;

        for (upto, current_index) in matching_overload_indexes.iter().enumerate() {
            if filter_remaining_overloads {
                self.overloads[*current_index].mark_as_unmatched_overload();
                continue;
            }
            let mut parameter_types = Vec::with_capacity(arguments.len());
            for argument_index in 0..arguments.len() {
                // The parameter types at the current argument index.
                let mut current_parameter_types = vec![];
                for overload_index in &matching_overload_indexes[..=upto] {
                    let overload = &self.overloads[*overload_index];
                    let Some(parameter_index) = overload.argument_parameters[argument_index] else {
                        // There is no parameter for this argument in this overload.
                        continue;
                    };
                    if !participating_parameter_indexes.contains(&parameter_index) {
                        // This parameter doesn't participate in the filtering process.
                        continue;
                    }
                    // TODO: For an unannotated `self` / `cls` parameter, the type should be
                    // `typing.Self` / `type[typing.Self]`
                    let parameter_type = overload.signature.parameters()[parameter_index]
                        .annotated_type()
                        .unwrap_or(Type::unknown());
                    current_parameter_types.push(parameter_type);
                }
                if current_parameter_types.is_empty() {
                    continue;
                }
                parameter_types.push(UnionType::from_elements(db, current_parameter_types));
            }
            if top_materialized_argument_type
                .is_assignable_to(db, TupleType::from_elements(db, parameter_types))
            {
                filter_remaining_overloads = true;
            }
        }

        // Once this filtering process is applied for all arguments, examine the return types of
        // the remaining overloads. If the resulting return types for all remaining overloads are
        // equivalent, proceed to step 6.
        let are_return_types_equivalent_for_all_matching_overloads = {
            let mut matching_overloads = self.matching_overloads();
            if let Some(first_overload_return_type) = matching_overloads
                .next()
                .map(|(_, overload)| overload.return_type())
            {
                matching_overloads.all(|(_, overload)| {
                    overload
                        .return_type()
                        .is_equivalent_to(db, first_overload_return_type)
                })
            } else {
                // No matching overload
                true
            }
        };

        if !are_return_types_equivalent_for_all_matching_overloads {
            // Overload matching is ambiguous.
            self.overload_call_return_type = Some(OverloadCallReturnType::Ambiguous);
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

    /// Returns the index of the matching overload in the form of [`MatchingOverloadIndex`].
    fn matching_overload_index(&self) -> MatchingOverloadIndex {
        let mut matching_overloads = self.matching_overloads();
        match matching_overloads.next() {
            None => MatchingOverloadIndex::None,
            Some((first, _)) => {
                if let Some((second, _)) = matching_overloads.next() {
                    let mut indexes = vec![first, second];
                    for (index, _) in matching_overloads {
                        indexes.push(index);
                    }
                    MatchingOverloadIndex::Multiple(indexes)
                } else {
                    MatchingOverloadIndex::Single(first)
                }
            }
        }
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

    /// Returns the return type of this call.
    ///
    /// For a valid call, this is the return type of either a successful argument type expansion of
    /// an overloaded function, or the return type of the first overload that the arguments matched
    /// against.
    ///
    /// For an invalid call to a non-overloaded function, this is the return type of the function.
    ///
    /// For an invalid call to an overloaded function, we return `Type::unknown`, since we cannot
    /// make any useful conclusions about which overload was intended to be called.
    pub(crate) fn return_type(&self) -> Type<'db> {
        if let Some(overload_call_return_type) = self.overload_call_return_type {
            return match overload_call_return_type {
                OverloadCallReturnType::ArgumentTypeExpansion(return_type) => return_type,
                OverloadCallReturnType::Ambiguous => Type::unknown(),
            };
        }
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
        context: &InferContext<'db, '_>,
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
                    None,
                );
            }
            _overloads => {
                // TODO: This should probably be adapted to handle more
                // types of callables[1]. At present, it just handles
                // standard function and method calls.
                //
                // [1]: https://github.com/astral-sh/ty/issues/274#issuecomment-2881856028
                let function_type_and_kind = match self.signature_type {
                    Type::FunctionLiteral(function) => Some((FunctionKind::Function, function)),
                    Type::BoundMethod(bound_method) => Some((
                        FunctionKind::BoundMethod,
                        bound_method.function(context.db()),
                    )),
                    Type::MethodWrapper(MethodWrapperKind::FunctionTypeDunderGet(function)) => {
                        Some((FunctionKind::MethodWrapper, function))
                    }
                    _ => None,
                };

                // If there is a single matching overload, the diagnostics should be reported
                // directly for that overload.
                if let Some(matching_overload_index) = self.matching_overload_index {
                    let callable_description =
                        CallableDescription::new(context.db(), self.signature_type);
                    let matching_overload =
                        function_type_and_kind.map(|(kind, function)| MatchingOverloadLiteral {
                            index: matching_overload_index,
                            kind,
                            function,
                        });
                    self.overloads[matching_overload_index].report_diagnostics(
                        context,
                        node,
                        self.signature_type,
                        callable_description.as_ref(),
                        union_diag,
                        matching_overload.as_ref(),
                    );
                    return;
                }

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
                if let Some((kind, function)) = function_type_and_kind {
                    let (overloads, implementation) =
                        function.overloads_and_implementation(context.db());

                    if let Some(spans) = overloads
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

                    for overload in overloads.iter().take(MAXIMUM_OVERLOADS) {
                        diag.info(format_args!(
                            "  {}",
                            overload.signature(context.db(), None).display(context.db())
                        ));
                    }
                    if overloads.len() > MAXIMUM_OVERLOADS {
                        diag.info(format_args!(
                            "... omitted {remaining} overloads",
                            remaining = overloads.len() - MAXIMUM_OVERLOADS
                        ));
                    }

                    if let Some(spans) =
                        implementation.and_then(|function| function.spans(context.db()))
                    {
                        let mut sub = SubDiagnostic::new(
                            Severity::Info,
                            "Overload implementation defined here",
                        );
                        sub.annotate(Annotation::primary(spans.signature));
                        diag.sub(sub);
                    }
                }

                if let Some(union_diag) = union_diag {
                    union_diag.add_union_context(context.db(), &mut diag);
                }
            }
        }
    }
}

impl<'a, 'db> IntoIterator for &'a CallableBinding<'db> {
    type Item = &'a Binding<'db>;
    type IntoIter = std::slice::Iter<'a, Binding<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.overloads.iter()
    }
}

#[derive(Debug, Copy, Clone)]
enum OverloadCallReturnType<'db> {
    ArgumentTypeExpansion(Type<'db>),
    Ambiguous,
}

#[derive(Debug)]
enum MatchingOverloadIndex {
    /// No matching overloads found.
    None,

    /// Exactly one matching overload found at the given index.
    Single(usize),

    /// Multiple matching overloads found at the given indexes.
    Multiple(Vec<usize>),
}

struct ArgumentMatcher<'a, 'db> {
    parameters: &'a Parameters<'db>,
    argument_forms: &'a mut [Option<ParameterForm>],
    conflicting_forms: &'a mut [bool],
    errors: &'a mut Vec<BindingError<'db>>,

    /// The parameter that each argument is matched with.
    argument_parameters: Vec<Option<usize>>,
    /// Whether each parameter has been matched with an argument.
    parameter_matched: Vec<bool>,
    next_positional: usize,
    first_excess_positional: Option<usize>,
    num_synthetic_args: usize,
}

impl<'a, 'db> ArgumentMatcher<'a, 'db> {
    fn new(
        arguments: &CallArguments,
        parameters: &'a Parameters<'db>,
        argument_forms: &'a mut [Option<ParameterForm>],
        conflicting_forms: &'a mut [bool],
        errors: &'a mut Vec<BindingError<'db>>,
    ) -> Self {
        Self {
            parameters,
            argument_forms,
            conflicting_forms,
            errors,
            argument_parameters: vec![None; arguments.len()],
            parameter_matched: vec![false; parameters.len()],
            next_positional: 0,
            first_excess_positional: None,
            num_synthetic_args: 0,
        }
    }

    fn get_argument_index(&self, argument_index: usize) -> Option<usize> {
        if argument_index >= self.num_synthetic_args {
            // Adjust the argument index to skip synthetic args, which don't appear at the call
            // site and thus won't be in the Call node arguments list.
            Some(argument_index - self.num_synthetic_args)
        } else {
            // we are erroring on a synthetic argument, we'll just emit the diagnostic on the
            // entire Call node, since there's no argument node for this argument at the call site
            None
        }
    }

    fn assign_argument(
        &mut self,
        argument_index: usize,
        argument: Argument<'a>,
        parameter_index: usize,
        parameter: &Parameter<'db>,
        positional: bool,
    ) {
        if !matches!(argument, Argument::Synthetic) {
            if let Some(existing) = self.argument_forms[argument_index - self.num_synthetic_args]
                .replace(parameter.form)
            {
                if existing != parameter.form {
                    self.conflicting_forms[argument_index - self.num_synthetic_args] = true;
                }
            }
        }
        if self.parameter_matched[parameter_index] {
            if !parameter.is_variadic() && !parameter.is_keyword_variadic() {
                self.errors.push(BindingError::ParameterAlreadyAssigned {
                    argument_index: self.get_argument_index(argument_index),
                    parameter: ParameterContext::new(parameter, parameter_index, positional),
                });
            }
        }
        self.argument_parameters[argument_index] = Some(parameter_index);
        self.parameter_matched[parameter_index] = true;
    }

    fn match_positional(
        &mut self,
        argument_index: usize,
        argument: Argument<'a>,
    ) -> Result<(), ()> {
        if matches!(argument, Argument::Synthetic) {
            self.num_synthetic_args += 1;
        }
        let Some((parameter_index, parameter)) = self
            .parameters
            .get_positional(self.next_positional)
            .map(|param| (self.next_positional, param))
            .or_else(|| self.parameters.variadic())
        else {
            self.first_excess_positional.get_or_insert(argument_index);
            self.next_positional += 1;
            return Err(());
        };
        self.next_positional += 1;
        self.assign_argument(
            argument_index,
            argument,
            parameter_index,
            parameter,
            !parameter.is_variadic(),
        );
        Ok(())
    }

    fn match_keyword(
        &mut self,
        argument_index: usize,
        argument: Argument<'a>,
        name: &str,
    ) -> Result<(), ()> {
        let Some((parameter_index, parameter)) = self
            .parameters
            .keyword_by_name(name)
            .or_else(|| self.parameters.keyword_variadic())
        else {
            self.errors.push(BindingError::UnknownArgument {
                argument_name: ast::name::Name::new(name),
                argument_index: self.get_argument_index(argument_index),
            });
            return Err(());
        };
        self.assign_argument(argument_index, argument, parameter_index, parameter, false);
        Ok(())
    }

    fn finish(self) -> Box<[Option<usize>]> {
        if let Some(first_excess_argument_index) = self.first_excess_positional {
            self.errors.push(BindingError::TooManyPositionalArguments {
                first_excess_argument_index: self.get_argument_index(first_excess_argument_index),
                expected_positional_count: self.parameters.positional().count(),
                provided_positional_count: self.next_positional,
            });
        }

        let mut missing = vec![];
        for (index, matched) in self.parameter_matched.iter().copied().enumerate() {
            if !matched {
                let param = &self.parameters[index];
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
            self.errors.push(BindingError::MissingArguments {
                parameters: ParameterContexts(missing),
            });
        }

        self.argument_parameters.into_boxed_slice()
    }
}

struct ArgumentTypeChecker<'a, 'db> {
    db: &'db dyn Db,
    signature: &'a Signature<'db>,
    arguments: &'a CallArguments<'a, 'db>,
    argument_parameters: &'a [Option<usize>],
    parameter_tys: &'a mut [Option<Type<'db>>],
    errors: &'a mut Vec<BindingError<'db>>,

    specialization: Option<Specialization<'db>>,
    inherited_specialization: Option<Specialization<'db>>,
}

impl<'a, 'db> ArgumentTypeChecker<'a, 'db> {
    fn new(
        db: &'db dyn Db,
        signature: &'a Signature<'db>,
        arguments: &'a CallArguments<'a, 'db>,
        argument_parameters: &'a [Option<usize>],
        parameter_tys: &'a mut [Option<Type<'db>>],
        errors: &'a mut Vec<BindingError<'db>>,
    ) -> Self {
        Self {
            db,
            signature,
            arguments,
            argument_parameters,
            parameter_tys,
            errors,
            specialization: None,
            inherited_specialization: None,
        }
    }

    fn enumerate_argument_types(
        &self,
    ) -> impl Iterator<Item = (usize, Option<usize>, Argument<'a>, Type<'db>)> + 'a {
        let mut iter = self.arguments.iter().enumerate();
        let mut num_synthetic_args = 0;
        std::iter::from_fn(move || {
            let (argument_index, (argument, argument_type)) = iter.next()?;
            let adjusted_argument_index = if matches!(argument, Argument::Synthetic) {
                // If we are erroring on a synthetic argument, we'll just emit the
                // diagnostic on the entire Call node, since there's no argument node for
                // this argument at the call site
                num_synthetic_args += 1;
                None
            } else {
                // Adjust the argument index to skip synthetic args, which don't appear at
                // the call site and thus won't be in the Call node arguments list.
                Some(argument_index - num_synthetic_args)
            };
            Some((
                argument_index,
                adjusted_argument_index,
                argument,
                argument_type.unwrap_or_else(Type::unknown),
            ))
        })
    }

    fn infer_specialization(&mut self) {
        if self.signature.generic_context.is_none()
            && self.signature.inherited_generic_context.is_none()
        {
            return;
        }

        let parameters = self.signature.parameters();
        let mut builder = SpecializationBuilder::new(self.db);
        for (argument_index, adjusted_argument_index, _, argument_type) in
            self.enumerate_argument_types()
        {
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
                    argument_index: adjusted_argument_index,
                });
            }
        }
        self.specialization = self.signature.generic_context.map(|gc| builder.build(gc));
        self.inherited_specialization = self.signature.inherited_generic_context.map(|gc| {
            // The inherited generic context is used when inferring the specialization of a generic
            // class from a constructor call. In this case (only), we promote any typevars that are
            // inferred as a literal to the corresponding instance type.
            builder
                .build(gc)
                .apply_type_mapping(self.db, &TypeMapping::PromoteLiterals)
        });
    }

    fn check_argument_type(
        &mut self,
        argument_index: usize,
        adjusted_argument_index: Option<usize>,
        argument: Argument<'a>,
        mut argument_type: Type<'db>,
    ) {
        let Some(parameter_index) = self.argument_parameters[argument_index] else {
            // There was an error with argument when matching parameters, so don't bother
            // type-checking it.
            return;
        };
        let parameters = self.signature.parameters();
        let parameter = &parameters[parameter_index];
        if let Some(mut expected_ty) = parameter.annotated_type() {
            if let Some(specialization) = self.specialization {
                argument_type = argument_type.apply_specialization(self.db, specialization);
                expected_ty = expected_ty.apply_specialization(self.db, specialization);
            }
            if let Some(inherited_specialization) = self.inherited_specialization {
                argument_type =
                    argument_type.apply_specialization(self.db, inherited_specialization);
                expected_ty = expected_ty.apply_specialization(self.db, inherited_specialization);
            }
            if !argument_type.is_assignable_to(self.db, expected_ty) {
                let positional = matches!(argument, Argument::Positional | Argument::Synthetic)
                    && !parameter.is_variadic();
                self.errors.push(BindingError::InvalidArgumentType {
                    parameter: ParameterContext::new(parameter, parameter_index, positional),
                    argument_index: adjusted_argument_index,
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
            let union = UnionType::from_elements(self.db, [existing, argument_type]);
            self.parameter_tys[parameter_index] = Some(union);
        }
    }

    fn check_argument_types(&mut self) {
        for (argument_index, adjusted_argument_index, argument, argument_type) in
            self.enumerate_argument_types()
        {
            self.check_argument_type(
                argument_index,
                adjusted_argument_index,
                argument,
                argument_type,
            );
        }
    }

    fn finish(self) -> (Option<Specialization<'db>>, Option<Specialization<'db>>) {
        (self.specialization, self.inherited_specialization)
    }
}

/// Binding information for one of the overloads of a callable.
#[derive(Debug)]
pub(crate) struct Binding<'db> {
    pub(crate) signature: Signature<'db>,

    /// The type that is (hopefully) callable.
    pub(crate) callable_type: Type<'db>,

    /// The type we'll use for error messages referring to details of the called signature. For
    /// calls to functions this will be the same as `callable_type`; for other callable instances
    /// it may be a `__call__` method.
    pub(crate) signature_type: Type<'db>,

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
    pub(crate) fn single(signature_type: Type<'db>, signature: Signature<'db>) -> Binding<'db> {
        Binding {
            signature,
            callable_type: signature_type,
            signature_type,
            return_ty: Type::unknown(),
            specialization: None,
            inherited_specialization: None,
            argument_parameters: Box::from([]),
            parameter_tys: Box::from([]),
            errors: vec![],
        }
    }

    fn replace_callable_type(&mut self, before: Type<'db>, after: Type<'db>) {
        if self.callable_type == before {
            self.callable_type = after;
        }
    }

    pub(crate) fn match_parameters(
        &mut self,
        arguments: &CallArguments<'_, 'db>,
        argument_forms: &mut [Option<ParameterForm>],
        conflicting_forms: &mut [bool],
    ) {
        let parameters = self.signature.parameters();
        let mut matcher = ArgumentMatcher::new(
            arguments,
            parameters,
            argument_forms,
            conflicting_forms,
            &mut self.errors,
        );
        for (argument_index, (argument, _)) in arguments.iter().enumerate() {
            match argument {
                Argument::Positional | Argument::Synthetic => {
                    let _ = matcher.match_positional(argument_index, argument);
                }
                Argument::Keyword(name) => {
                    let _ = matcher.match_keyword(argument_index, argument, name);
                }
                Argument::Variadic | Argument::Keywords => {
                    // TODO
                    continue;
                }
            }
        }
        self.return_ty = self.signature.return_ty.unwrap_or(Type::unknown());
        self.parameter_tys = vec![None; parameters.len()].into_boxed_slice();
        self.argument_parameters = matcher.finish();
    }

    fn check_types(&mut self, db: &'db dyn Db, arguments: &CallArguments<'_, 'db>) {
        let mut checker = ArgumentTypeChecker::new(
            db,
            &self.signature,
            arguments,
            &self.argument_parameters,
            &mut self.parameter_tys,
            &mut self.errors,
        );

        // If this overload is generic, first see if we can infer a specialization of the function
        // from the arguments that were passed in.
        checker.infer_specialization();

        checker.check_argument_types();
        (self.specialization, self.inherited_specialization) = checker.finish();
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
        argument_types: &'a CallArguments<'a, 'db>,
        parameter_index: usize,
    ) -> impl Iterator<Item = (Argument<'a>, Type<'db>)> + 'a {
        argument_types
            .iter()
            .zip(&self.argument_parameters)
            .filter(move |(_, argument_parameter)| {
                argument_parameter.is_some_and(|ap| ap == parameter_index)
            })
            .map(|((argument, argument_type), _)| {
                (argument, argument_type.unwrap_or_else(Type::unknown))
            })
    }

    /// Mark this overload binding as an unmatched overload.
    fn mark_as_unmatched_overload(&mut self) {
        self.errors.push(BindingError::UnmatchedOverload);
    }

    fn report_diagnostics(
        &self,
        context: &InferContext<'db, '_>,
        node: ast::AnyNodeRef,
        callable_ty: Type<'db>,
        callable_description: Option<&CallableDescription>,
        union_diag: Option<&UnionDiagnostic<'_, '_>>,
        matching_overload: Option<&MatchingOverloadLiteral<'db>>,
    ) {
        for error in &self.errors {
            error.report_diagnostic(
                context,
                node,
                callable_ty,
                callable_description,
                union_diag,
                matching_overload,
            );
        }
    }

    fn as_result(&self) -> Result<(), CallErrorKind> {
        if !self.errors.is_empty() {
            return Err(CallErrorKind::BindingError);
        }
        Ok(())
    }

    fn snapshot(&self) -> BindingSnapshot<'db> {
        BindingSnapshot {
            return_ty: self.return_ty,
            specialization: self.specialization,
            inherited_specialization: self.inherited_specialization,
            argument_parameters: self.argument_parameters.clone(),
            parameter_tys: self.parameter_tys.clone(),
            errors: self.errors.clone(),
        }
    }

    fn restore(&mut self, snapshot: BindingSnapshot<'db>) {
        let BindingSnapshot {
            return_ty,
            specialization,
            inherited_specialization,
            argument_parameters,
            parameter_tys,
            errors,
        } = snapshot;

        self.return_ty = return_ty;
        self.specialization = specialization;
        self.inherited_specialization = inherited_specialization;
        self.argument_parameters = argument_parameters;
        self.parameter_tys = parameter_tys;
        self.errors = errors;
    }

    /// Returns a vector where each index corresponds to an argument position,
    /// and the value is the parameter index that argument maps to (if any).
    pub(crate) fn argument_to_parameter_mapping(&self) -> &[Option<usize>] {
        &self.argument_parameters
    }
}

#[derive(Clone, Debug)]
struct BindingSnapshot<'db> {
    return_ty: Type<'db>,
    specialization: Option<Specialization<'db>>,
    inherited_specialization: Option<Specialization<'db>>,
    argument_parameters: Box<[Option<usize>]>,
    parameter_tys: Box<[Option<Type<'db>>]>,
    errors: Vec<BindingError<'db>>,
}

#[derive(Clone, Debug)]
struct CallableBindingSnapshot<'db> {
    overload_return_type: Option<OverloadCallReturnType<'db>>,

    /// Represents the snapshot of the matched overload bindings.
    ///
    /// The reason that this only contains the matched overloads are:
    /// 1. Avoid creating snapshots for the overloads that have been filtered by the arity check
    /// 2. Avoid duplicating errors when merging the snapshots on a successful evaluation of all
    ///    the expanded argument lists
    matching_overloads: Vec<(usize, BindingSnapshot<'db>)>,
}

impl<'db> CallableBindingSnapshot<'db> {
    /// Update the state of the matched overload bindings in this snapshot with the current
    /// state in the given `binding`.
    fn update(&mut self, binding: &CallableBinding<'db>) {
        // Here, the `snapshot` is the state of this binding for the previous argument list and
        // `binding` would contain the state after evaluating the current argument list.
        for (snapshot, binding) in self
            .matching_overloads
            .iter_mut()
            .map(|(index, snapshot)| (snapshot, &binding.overloads[*index]))
        {
            if binding.errors.is_empty() {
                // If the binding has no errors, this means that the current argument list was
                // evaluated successfully and this is the matching overload.
                //
                // Clear the errors from the snapshot of this overload to signal this change ...
                snapshot.errors.clear();

                // ... and update the snapshot with the current state of the binding.
                snapshot.return_ty = binding.return_ty;
                snapshot.specialization = binding.specialization;
                snapshot.inherited_specialization = binding.inherited_specialization;
                snapshot
                    .argument_parameters
                    .clone_from(&binding.argument_parameters);
                snapshot.parameter_tys.clone_from(&binding.parameter_tys);
            }

            // If the errors in the snapshot was empty, then this binding is the matching overload
            // for a previously evaluated argument list. This means that we don't need to change
            // any information for an already matched overload binding.
            //
            // If it does have errors, we could extend it with the errors from evaluating the
            // current argument list. Arguably, this isn't required, since the errors in the
            // snapshot should already signal that this is an unmatched overload which is why we
            // don't do it. Similarly, due to this being an unmatched overload, there's no point in
            // updating the binding state.
        }
    }
}

/// A helper to take snapshots of the matched overload bindings for the current state of the
/// bindings.
struct CallableBindingSnapshotter(Vec<usize>);

impl CallableBindingSnapshotter {
    /// Creates a new snapshotter for the given indexes of the matched overloads.
    fn new(indexes: Vec<usize>) -> Self {
        debug_assert!(indexes.len() > 1);
        CallableBindingSnapshotter(indexes)
    }

    /// Takes a snapshot of the current state of the matched overload bindings.
    ///
    /// # Panics
    ///
    /// Panics if the indexes of the matched overloads are not valid for the given binding.
    fn take<'db>(&self, binding: &CallableBinding<'db>) -> CallableBindingSnapshot<'db> {
        CallableBindingSnapshot {
            overload_return_type: binding.overload_call_return_type,
            matching_overloads: self
                .0
                .iter()
                .map(|index| (*index, binding.overloads[*index].snapshot()))
                .collect(),
        }
    }

    /// Restores the state of the matched overload bindings from the given snapshot.
    fn restore<'db>(
        &self,
        binding: &mut CallableBinding<'db>,
        snapshot: CallableBindingSnapshot<'db>,
    ) {
        debug_assert_eq!(self.0.len(), snapshot.matching_overloads.len());
        binding.overload_call_return_type = snapshot.overload_return_type;
        for (index, snapshot) in snapshot.matching_overloads {
            binding.overloads[index].restore(snapshot);
        }
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
    /// This overload binding of the callable does not match the arguments.
    // TODO: We could expand this with an enum to specify why the overload is unmatched.
    UnmatchedOverload,
}

impl<'db> BindingError<'db> {
    fn report_diagnostic(
        &self,
        context: &InferContext<'db, '_>,
        node: ast::AnyNodeRef,
        callable_ty: Type<'db>,
        callable_description: Option<&CallableDescription>,
        union_diag: Option<&UnionDiagnostic<'_, '_>>,
        matching_overload: Option<&MatchingOverloadLiteral<'_>>,
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

                if let Some(matching_overload) = matching_overload {
                    if let Some((name_span, parameter_span)) =
                        matching_overload.get(context.db()).and_then(|overload| {
                            overload.parameter_span(context.db(), Some(parameter.index))
                        })
                    {
                        let mut sub =
                            SubDiagnostic::new(Severity::Info, "Matching overload defined here");
                        sub.annotate(Annotation::primary(name_span));
                        sub.annotate(
                            Annotation::secondary(parameter_span)
                                .message("Parameter declared here"),
                        );
                        diag.sub(sub);
                        diag.info(format_args!(
                            "Non-matching overloads for {} `{}`:",
                            matching_overload.kind,
                            matching_overload.function.name(context.db())
                        ));
                        let (overloads, _) = matching_overload
                            .function
                            .overloads_and_implementation(context.db());
                        for (overload_index, overload) in
                            overloads.iter().enumerate().take(MAXIMUM_OVERLOADS)
                        {
                            if overload_index == matching_overload.index {
                                continue;
                            }
                            diag.info(format_args!(
                                "  {}",
                                overload.signature(context.db(), None).display(context.db())
                            ));
                        }
                        if overloads.len() > MAXIMUM_OVERLOADS {
                            diag.info(format_args!(
                                "... omitted {remaining} overloads",
                                remaining = overloads.len() - MAXIMUM_OVERLOADS
                            ));
                        }
                    }
                } else if let Some((name_span, parameter_span)) =
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
                    let module = parsed_module(context.db(), typevar_definition.file(context.db()))
                        .load(context.db());
                    let typevar_range = typevar_definition.full_range(context.db(), &module);
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

            Self::UnmatchedOverload => {}
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

/// Represents the matching overload of a function literal that was found via the overload call
/// evaluation algorithm.
struct MatchingOverloadLiteral<'db> {
    /// The position of the matching overload in the list of overloads.
    index: usize,
    /// The kind of function this overload is for.
    kind: FunctionKind,
    /// The function literal that this overload belongs to.
    ///
    /// This is used to retrieve the overload at the given index.
    function: FunctionType<'db>,
}

impl<'db> MatchingOverloadLiteral<'db> {
    /// Returns the [`OverloadLiteral`] representing this matching overload.
    fn get(&self, db: &'db dyn Db) -> Option<OverloadLiteral<'db>> {
        let (overloads, _) = self.function.overloads_and_implementation(db);

        // TODO: This should actually be safe to index directly but isn't so as of this writing.
        // The main reason is that we've custom overload signatures that are constructed manually
        // and does not belong to any file. For example, the `__get__` method of a function literal
        // has a custom overloaded signature. So, when we try to retrieve the actual overloads
        // above, we get an empty list of overloads because the implementation of that method
        // relies on it existing in the file.
        overloads.get(self.index).copied()
    }
}

#[derive(Clone, Copy, Debug)]
enum FunctionKind {
    Function,
    BoundMethod,
    MethodWrapper,
}

impl fmt::Display for FunctionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FunctionKind::Function => write!(f, "function"),
            FunctionKind::BoundMethod => write!(f, "bound method"),
            FunctionKind::MethodWrapper => write!(f, "method wrapper `__get__` of function"),
        }
    }
}

// When the number of unmatched overloads exceeds this number, we stop printing them to avoid
// excessive output.
//
// An example of a routine with many many overloads:
// https://github.com/henribru/google-api-python-client-stubs/blob/master/googleapiclient-stubs/discovery.pyi
const MAXIMUM_OVERLOADS: usize = 50;
