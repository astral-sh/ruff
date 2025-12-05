//! When analyzing a call site, we create _bindings_, which match and type-check the actual
//! arguments against the parameters of the callable. Like with
//! [signatures][crate::types::signatures], we have to handle the fact that the callable might be a
//! union of types, each of which might contain multiple overloads.

use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt;

use itertools::{Either, Itertools};
use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::{SmallVec, smallvec, smallvec_inline};

use super::{Argument, CallArguments, CallError, CallErrorKind, InferContext, Signature, Type};
use crate::Program;
use crate::db::Db;
use crate::dunder_all::dunder_all_names;
use crate::place::{Definedness, Place};
use crate::types::call::arguments::{Expansion, is_expandable_type};
use crate::types::constraints::ConstraintSet;
use crate::types::diagnostic::{
    CALL_NON_CALLABLE, CONFLICTING_ARGUMENT_FORMS, INVALID_ARGUMENT_TYPE, MISSING_ARGUMENT,
    NO_MATCHING_OVERLOAD, PARAMETER_ALREADY_ASSIGNED, POSITIONAL_ONLY_PARAMETER_AS_KWARG,
    TOO_MANY_POSITIONAL_ARGUMENTS, UNKNOWN_ARGUMENT,
};
use crate::types::enums::is_enum_class;
use crate::types::function::{
    DataclassTransformerFlags, DataclassTransformerParams, FunctionType, KnownFunction,
    OverloadLiteral,
};
use crate::types::generics::{
    InferableTypeVars, Specialization, SpecializationBuilder, SpecializationError,
};
use crate::types::signatures::{Parameter, ParameterForm, ParameterKind, Parameters};
use crate::types::tuple::{TupleLength, TupleSpec, TupleType};
use crate::types::{
    BoundMethodType, BoundTypeVarIdentity, BoundTypeVarInstance, CallableSignature,
    CallableTypeKind, ClassLiteral, DATACLASS_FLAGS, DataclassFlags, DataclassParams,
    FieldInstance, KnownBoundMethodType, KnownClass, KnownInstanceType, MemberLookupPolicy,
    NominalInstanceType, PropertyInstanceType, SpecialFormType, TrackedConstraintSet,
    TypeAliasType, TypeContext, TypeVarVariance, UnionBuilder, UnionType, WrapperDescriptorKind,
    enums, list_members, todo_type,
};
use ruff_db::diagnostic::{Annotation, Diagnostic, SubDiagnostic, SubDiagnosticSeverity};
use ruff_python_ast::{self as ast, ArgOrKeyword, PythonVersion};

/// Binding information for a possible union of callables. At a call site, the arguments must be
/// compatible with _all_ of the types in the union for the call to be valid.
///
/// It's guaranteed that the wrapped bindings have no errors.
#[derive(Debug, Clone)]
pub(crate) struct Bindings<'db> {
    /// The type that is (hopefully) callable.
    callable_type: Type<'db>,

    /// The type of the instance being constructed, if this signature is for a constructor.
    constructor_instance_type: Option<Type<'db>>,

    /// By using `SmallVec`, we avoid an extra heap allocation for the common case of a non-union
    /// type.
    elements: SmallVec<[CallableBinding<'db>; 1]>,

    /// Whether each argument will be used as a value and/or a type form in this call.
    argument_forms: ArgumentForms,
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
            argument_forms: ArgumentForms::new(0),
            constructor_instance_type: None,
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

    pub(crate) fn with_constructor_instance_type(
        mut self,
        constructor_instance_type: Type<'db>,
    ) -> Self {
        self.constructor_instance_type = Some(constructor_instance_type);

        for binding in &mut self.elements {
            binding.constructor_instance_type = Some(constructor_instance_type);
            for binding in &mut binding.overloads {
                binding.constructor_instance_type = Some(constructor_instance_type);
            }
        }

        self
    }

    pub(crate) fn set_dunder_call_is_possibly_unbound(&mut self) {
        for binding in &mut self.elements {
            binding.dunder_call_is_possibly_unbound = true;
        }
    }

    pub(crate) fn argument_forms(&self) -> &[Option<ParameterForm>] {
        &self.argument_forms.values
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, CallableBinding<'db>> {
        self.elements.iter()
    }

    pub(crate) fn map(self, f: impl Fn(CallableBinding<'db>) -> CallableBinding<'db>) -> Self {
        Self {
            callable_type: self.callable_type,
            argument_forms: self.argument_forms,
            constructor_instance_type: self.constructor_instance_type,
            elements: self.elements.into_iter().map(f).collect(),
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
    pub(crate) fn match_parameters(
        mut self,
        db: &'db dyn Db,
        arguments: &CallArguments<'_, 'db>,
    ) -> Self {
        let mut argument_forms = ArgumentForms::new(arguments.len());
        for binding in &mut self.elements {
            binding.match_parameters(db, arguments, &mut argument_forms);
        }
        argument_forms.shrink_to_fit();
        self.argument_forms = argument_forms;
        self
    }

    /// Verify that the type of each argument is assignable to type of the parameter that it was
    /// matched to.
    ///
    /// You must provide an `argument_types` that was created from the same `arguments` that you
    /// provided to [`match_parameters`][Self::match_parameters].
    ///
    /// The type context of the call expression is also used to infer the specialization of generic
    /// calls.
    ///
    /// We update the bindings to include the return type of the call, the bound types for all
    /// parameters, and any errors resulting from binding the call, all for each union element and
    /// overload (if any).
    pub(crate) fn check_types(
        mut self,
        db: &'db dyn Db,
        argument_types: &CallArguments<'_, 'db>,
        call_expression_tcx: TypeContext<'db>,
        dataclass_field_specifiers: &[Type<'db>],
    ) -> Result<Self, CallError<'db>> {
        match self.check_types_impl(
            db,
            argument_types,
            call_expression_tcx,
            dataclass_field_specifiers,
        ) {
            Ok(()) => Ok(self),
            Err(err) => Err(CallError(err, Box::new(self))),
        }
    }

    pub(crate) fn check_types_impl(
        &mut self,
        db: &'db dyn Db,
        argument_types: &CallArguments<'_, 'db>,
        call_expression_tcx: TypeContext<'db>,
        dataclass_field_specifiers: &[Type<'db>],
    ) -> Result<(), CallErrorKind> {
        for element in &mut self.elements {
            if let Some(mut updated_argument_forms) =
                element.check_types(db, argument_types, call_expression_tcx)
            {
                // If this element returned a new set of argument forms (indicating successful
                // argument type expansion), update the `Bindings` with these forms.
                updated_argument_forms.shrink_to_fit();
                self.argument_forms = updated_argument_forms;
            }
        }

        self.evaluate_known_cases(db, dataclass_field_specifiers);

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
        if self.argument_forms.conflicting.contains(&true) {
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
            Ok(())
        } else if any_binding_error {
            Err(CallErrorKind::BindingError)
        } else if all_not_callable {
            Err(CallErrorKind::NotCallable)
        } else {
            Err(CallErrorKind::PossiblyNotCallable)
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

    pub(crate) fn constructor_instance_type(&self) -> Option<Type<'db>> {
        self.constructor_instance_type
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

        for (index, conflicting_form) in self.argument_forms.conflicting.iter().enumerate() {
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
    fn evaluate_known_cases(&mut self, db: &'db dyn Db, dataclass_field_specifiers: &[Type<'db>]) {
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
                    Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(
                        function,
                    )) => {
                        if function.is_classmethod(db) {
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
                        } else if function.is_staticmethod(db) {
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
                            if function.is_classmethod(db) {
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
                            } else if function.is_staticmethod(db) {
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
                                    .as_function_literal()
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
                                    .and_then(Type::as_function_literal)
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
                                        overload.set_return_type(Type::heterogeneous_tuple(
                                            db,
                                            typevar.constraints(db).into_iter().flatten(),
                                        ));
                                    }
                                    Some("__default__") => {
                                        overload.set_return_type(
                                            typevar.default_type(db).unwrap_or_else(|| {
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
                                    overload
                                        .errors
                                        .push(BindingError::PropertyHasNoSetter(*property));
                                    overload.set_return_type(Type::Never);
                                }
                            }
                            _ => {}
                        }
                    }

                    Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderGet(property)) => {
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
                                overload
                                    .errors
                                    .push(BindingError::PropertyHasNoSetter(*property));
                            }
                        }
                    }

                    Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderSet(property)) => {
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
                                overload
                                    .errors
                                    .push(BindingError::PropertyHasNoSetter(property));
                            }
                        }
                    }

                    Type::KnownBoundMethod(KnownBoundMethodType::StrStartswith(literal)) => {
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

                    // TODO: This branch can be removed once https://github.com/astral-sh/ty/issues/501 is resolved
                    Type::BoundMethod(bound_method)
                        if bound_method.function(db).name(db) == "__iter__"
                            && is_enum_class(db, bound_method.self_instance(db)) =>
                    {
                        if let Some(enum_instance) = bound_method.self_instance(db).to_instance(db)
                        {
                            overload.set_return_type(
                                KnownClass::Iterator.to_specialized_instance(db, [enum_instance]),
                            );
                        }
                    }

                    function @ Type::FunctionLiteral(function_type)
                        if dataclass_field_specifiers.contains(&function)
                            || function_type.is_known(db, KnownFunction::Field) =>
                    {
                        let has_default_value = overload
                            .parameter_type_by_name("default", false)
                            .is_ok_and(|ty| ty.is_some())
                            || overload
                                .parameter_type_by_name("default_factory", false)
                                .is_ok_and(|ty| ty.is_some())
                            || overload
                                .parameter_type_by_name("factory", false)
                                .is_ok_and(|ty| ty.is_some());

                        let init = overload
                            .parameter_type_by_name("init", true)
                            .unwrap_or(None);
                        let kw_only = overload
                            .parameter_type_by_name("kw_only", true)
                            .unwrap_or(None);
                        let alias = overload
                            .parameter_type_by_name("alias", true)
                            .unwrap_or(None);

                        // `dataclasses.field` and field-specifier functions of commonly used
                        // libraries like `pydantic`, `attrs`, and `SQLAlchemy` all return
                        // the default type for the field (or `Any`) instead of an actual `Field`
                        // instance, even if this is not what happens at runtime (see also below).
                        // We still make use of this fact and pretend that all field specifiers
                        // return the type of the default value:
                        let default_ty = if has_default_value {
                            Some(overload.return_ty)
                        } else {
                            None
                        };

                        let init = init
                            .map(|init| !init.bool(db).is_always_false())
                            .unwrap_or(true);

                        let kw_only = if Program::get(db).python_version(db) >= PythonVersion::PY310
                        {
                            match kw_only {
                                // We are more conservative here when turning the type for `kw_only`
                                // into a bool, because a field specifier in a stub might use
                                // `kw_only: bool = ...` and the truthiness of `...` is always true.
                                // This is different from `init` above because may need to fall back
                                // to `kw_only_default`, whereas `init_default` does not exist.
                                Some(Type::BooleanLiteral(yes)) => Some(yes),
                                _ => None,
                            }
                        } else {
                            None
                        };

                        let alias = alias
                            .and_then(Type::as_string_literal)
                            .map(|literal| Box::from(literal.value(db)));

                        // `typeshed` pretends that `dataclasses.field()` returns the type of the
                        // default value directly. At runtime, however, this function returns an
                        // instance of `dataclasses.Field`. We also model it this way and return
                        // a known-instance type with information about the field. The drawback
                        // of this approach is that we need to pretend that instances of `Field`
                        // are assignable to `T` if the default type of the field is assignable
                        // to `T`. Otherwise, we would error on `name: str = field(default="")`.
                        overload.set_return_type(Type::KnownInstance(KnownInstanceType::Field(
                            FieldInstance::new(db, default_ty, init, kw_only, alias),
                        )));
                    }

                    Type::FunctionLiteral(function_type) => match function_type.known(db) {
                        Some(KnownFunction::IsEquivalentTo) => {
                            if let [Some(ty_a), Some(ty_b)] = overload.parameter_types() {
                                let constraints =
                                    ty_a.when_equivalent_to(db, *ty_b, InferableTypeVars::None);
                                let tracked = TrackedConstraintSet::new(db, constraints);
                                overload.set_return_type(Type::KnownInstance(
                                    KnownInstanceType::ConstraintSet(tracked),
                                ));
                            }
                        }

                        Some(KnownFunction::IsSubtypeOf) => {
                            if let [Some(ty_a), Some(ty_b)] = overload.parameter_types() {
                                let constraints =
                                    ty_a.when_subtype_of(db, *ty_b, InferableTypeVars::None);
                                let tracked = TrackedConstraintSet::new(db, constraints);
                                overload.set_return_type(Type::KnownInstance(
                                    KnownInstanceType::ConstraintSet(tracked),
                                ));
                            }
                        }

                        Some(KnownFunction::IsAssignableTo) => {
                            if let [Some(ty_a), Some(ty_b)] = overload.parameter_types() {
                                let constraints =
                                    ty_a.when_assignable_to(db, *ty_b, InferableTypeVars::None);
                                let tracked = TrackedConstraintSet::new(db, constraints);
                                overload.set_return_type(Type::KnownInstance(
                                    KnownInstanceType::ConstraintSet(tracked),
                                ));
                            }
                        }

                        Some(KnownFunction::IsDisjointFrom) => {
                            if let [Some(ty_a), Some(ty_b)] = overload.parameter_types() {
                                let constraints =
                                    ty_a.when_disjoint_from(db, *ty_b, InferableTypeVars::None);
                                let tracked = TrackedConstraintSet::new(db, constraints);
                                overload.set_return_type(Type::KnownInstance(
                                    KnownInstanceType::ConstraintSet(tracked),
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
                                let wrap_generic_context = |generic_context| {
                                    Type::KnownInstance(KnownInstanceType::GenericContext(
                                        generic_context,
                                    ))
                                };

                                let signature_generic_context =
                                    |signature: &CallableSignature<'db>| {
                                        UnionType::try_from_elements(
                                            db,
                                            signature.overloads.iter().map(|signature| {
                                                signature.generic_context.map(wrap_generic_context)
                                            }),
                                        )
                                    };

                                let generic_context_for_simple_type = |ty: Type<'db>| match ty {
                                    Type::ClassLiteral(class) => {
                                        class.generic_context(db).map(wrap_generic_context)
                                    }

                                    Type::FunctionLiteral(function) => {
                                        signature_generic_context(function.signature(db))
                                    }

                                    Type::BoundMethod(bound_method) => signature_generic_context(
                                        bound_method.function(db).signature(db),
                                    ),

                                    Type::Callable(callable) => {
                                        signature_generic_context(callable.signatures(db))
                                    }

                                    Type::KnownInstance(KnownInstanceType::TypeAliasType(
                                        TypeAliasType::PEP695(alias),
                                    )) => alias.generic_context(db).map(wrap_generic_context),

                                    _ => None,
                                };

                                let generic_context = match ty {
                                    Type::Union(union_type) => UnionType::try_from_elements(
                                        db,
                                        union_type
                                            .elements(db)
                                            .iter()
                                            .map(|ty| generic_context_for_simple_type(*ty)),
                                    ),
                                    _ => generic_context_for_simple_type(*ty),
                                };

                                overload.set_return_type(
                                    generic_context.unwrap_or_else(|| Type::none(db)),
                                );
                            }
                        }

                        Some(KnownFunction::IntoCallable) => {
                            let [Some(ty)] = overload.parameter_types() else {
                                continue;
                            };
                            let Some(callables) = ty.try_upcast_to_callable(db) else {
                                continue;
                            };
                            overload.set_return_type(callables.into_type(db));
                        }

                        Some(KnownFunction::DunderAllNames) => {
                            if let [Some(ty)] = overload.parameter_types() {
                                overload.set_return_type(match ty {
                                    Type::ModuleLiteral(module_literal) => {
                                        let all_names = module_literal
                                            .module(db)
                                            .file(db)
                                            .map(|file| dunder_all_names(db, file))
                                            .unwrap_or_default();
                                        match all_names {
                                            Some(names) => {
                                                let mut names = names.iter().collect::<Vec<_>>();
                                                names.sort();
                                                Type::heterogeneous_tuple(
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
                                            Type::heterogeneous_tuple(
                                                db,
                                                metadata
                                                    .members
                                                    .keys()
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
                                overload.set_return_type(Type::heterogeneous_tuple(
                                    db,
                                    list_members::all_members(db, *ty)
                                        .into_iter()
                                        .sorted()
                                        .map(|member| Type::string_literal(db, &member.name)),
                                ));
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
                                // We evaluate this to `Literal[True]` only if the runtime function `typing.is_protocol`
                                // would return `True` for the given type. Internally we consider `SupportsAbs[int]` to
                                // be a "(specialised) protocol class", but `typing.is_protocol(SupportsAbs[int])` returns
                                // `False` at runtime, so we do not set the return type to `Literal[True]` in this case.
                                overload.set_return_type(Type::BooleanLiteral(
                                    ty.as_class_literal()
                                        .is_some_and(|class| class.is_protocol(db)),
                                ));
                            }
                        }

                        Some(KnownFunction::GetProtocolMembers) => {
                            // Similarly to `is_protocol`, we only evaluate to this a frozenset of literal strings if a
                            // class-literal is passed in, not if a generic alias is passed in, to emulate the behaviour
                            // of `typing.get_protocol_members` at runtime.
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

                            let Some(attr_name) = attr_name.as_string_literal() else {
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
                                    Place::Defined(ty, _, Definedness::AlwaysDefined) => {
                                        if ty.is_dynamic() {
                                            // Here, we attempt to model the fact that an attribute lookup on
                                            // a dynamic type could fail

                                            union_with_default(ty)
                                        } else {
                                            ty
                                        }
                                    }
                                    Place::Defined(ty, _, Definedness::PossiblyUndefined) => {
                                        union_with_default(ty)
                                    }
                                    Place::Undefined => default,
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
                                let mut flags = DataclassFlags::empty();

                                if to_bool(init, true) {
                                    flags |= DataclassFlags::INIT;
                                }
                                if to_bool(repr, true) {
                                    flags |= DataclassFlags::REPR;
                                }
                                if to_bool(eq, true) {
                                    flags |= DataclassFlags::EQ;
                                }
                                if to_bool(order, false) {
                                    flags |= DataclassFlags::ORDER;
                                }
                                if to_bool(unsafe_hash, false) {
                                    flags |= DataclassFlags::UNSAFE_HASH;
                                }
                                if to_bool(frozen, false) {
                                    flags |= DataclassFlags::FROZEN;
                                }
                                if to_bool(match_args, true) {
                                    if Program::get(db).python_version(db) >= PythonVersion::PY310 {
                                        flags |= DataclassFlags::MATCH_ARGS;
                                    } else {
                                        // TODO: emit diagnostic
                                    }
                                }
                                if to_bool(kw_only, false) {
                                    if Program::get(db).python_version(db) >= PythonVersion::PY310 {
                                        flags |= DataclassFlags::KW_ONLY;
                                    } else {
                                        // TODO: emit diagnostic
                                    }
                                }
                                if to_bool(slots, false) {
                                    if Program::get(db).python_version(db) >= PythonVersion::PY310 {
                                        flags |= DataclassFlags::SLOTS;
                                    } else {
                                        // TODO: emit diagnostic
                                    }
                                }
                                if to_bool(weakref_slot, false) {
                                    if Program::get(db).python_version(db) >= PythonVersion::PY311 {
                                        flags |= DataclassFlags::WEAKREF_SLOT;
                                    } else {
                                        // TODO: emit diagnostic
                                    }
                                }

                                let params = DataclassParams::from_flags(db, flags);

                                overload.set_return_type(Type::DataclassDecorator(params));
                            }

                            // `dataclass` being used as a non-decorator
                            if let [Some(Type::ClassLiteral(class_literal))] =
                                overload.parameter_types()
                            {
                                let params = DataclassParams::default_params(db);
                                overload.set_return_type(Type::from(ClassLiteral::new(
                                    db,
                                    class_literal.name(db),
                                    class_literal.body_scope(db),
                                    class_literal.known(db),
                                    class_literal.deprecated(db),
                                    class_literal.type_check_only(db),
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
                                field_specifiers,
                                _kwargs,
                            ] = overload.parameter_types()
                            {
                                let mut flags = DataclassTransformerFlags::empty();

                                if to_bool(eq_default, true) {
                                    flags |= DataclassTransformerFlags::EQ_DEFAULT;
                                }
                                if to_bool(order_default, false) {
                                    flags |= DataclassTransformerFlags::ORDER_DEFAULT;
                                }
                                if to_bool(kw_only_default, false) {
                                    flags |= DataclassTransformerFlags::KW_ONLY_DEFAULT;
                                }
                                if to_bool(frozen_default, false) {
                                    flags |= DataclassTransformerFlags::FROZEN_DEFAULT;
                                }

                                let field_specifiers: Box<[Type<'db>]> = field_specifiers
                                    .map(|tuple_type| {
                                        tuple_type
                                            .exact_tuple_instance_spec(db)
                                            .iter()
                                            .flat_map(|tuple_spec| tuple_spec.fixed_elements())
                                            .copied()
                                            .collect()
                                    })
                                    .unwrap_or_default();

                                let params =
                                    DataclassTransformerParams::new(db, flags, field_specifiers);

                                overload.set_return_type(Type::DataclassTransformer(params));
                            }
                        }

                        Some(KnownFunction::NamedTuple) => {
                            overload
                                .set_return_type(todo_type!("Support for functional `namedtuple`"));
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

                                            let dataclass_params =
                                                DataclassParams::from_transformer_params(
                                                    db, params,
                                                );
                                            let mut flags = dataclass_params.flags(db);

                                            for (param, flag) in DATACLASS_FLAGS {
                                                if let Ok(Some(Type::BooleanLiteral(value))) =
                                                    overload.parameter_type_by_name(param, false)
                                                {
                                                    flags.set(*flag, value);
                                                }
                                            }

                                            Type::DataclassDecorator(DataclassParams::new(
                                                db,
                                                flags,
                                                dataclass_params.field_specifiers(db),
                                            ))
                                        },
                                    )
                                })
                                .last();

                            if let Some(return_type) = return_type {
                                overload.set_return_type(return_type);
                            }
                        }
                    },

                    Type::KnownBoundMethod(KnownBoundMethodType::ConstraintSetRange) => {
                        let [Some(lower), Some(Type::TypeVar(typevar)), Some(upper)] =
                            overload.parameter_types()
                        else {
                            return;
                        };
                        let constraints = ConstraintSet::range(db, *lower, *typevar, *upper);
                        let tracked = TrackedConstraintSet::new(db, constraints);
                        overload.set_return_type(Type::KnownInstance(
                            KnownInstanceType::ConstraintSet(tracked),
                        ));
                    }

                    Type::KnownBoundMethod(KnownBoundMethodType::ConstraintSetAlways) => {
                        if !overload.parameter_types().is_empty() {
                            return;
                        }
                        let constraints = ConstraintSet::from(true);
                        let tracked = TrackedConstraintSet::new(db, constraints);
                        overload.set_return_type(Type::KnownInstance(
                            KnownInstanceType::ConstraintSet(tracked),
                        ));
                    }

                    Type::KnownBoundMethod(KnownBoundMethodType::ConstraintSetNever) => {
                        if !overload.parameter_types().is_empty() {
                            return;
                        }
                        let constraints = ConstraintSet::from(false);
                        let tracked = TrackedConstraintSet::new(db, constraints);
                        overload.set_return_type(Type::KnownInstance(
                            KnownInstanceType::ConstraintSet(tracked),
                        ));
                    }

                    Type::KnownBoundMethod(
                        KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(tracked),
                    ) => {
                        let [Some(ty_a), Some(ty_b)] = overload.parameter_types() else {
                            continue;
                        };

                        let result = ty_a.when_subtype_of_assuming(
                            db,
                            *ty_b,
                            tracked.constraints(db),
                            InferableTypeVars::None,
                        );
                        let tracked = TrackedConstraintSet::new(db, result);
                        overload.set_return_type(Type::KnownInstance(
                            KnownInstanceType::ConstraintSet(tracked),
                        ));
                    }

                    Type::KnownBoundMethod(KnownBoundMethodType::ConstraintSetSatisfies(
                        tracked,
                    )) => {
                        let [Some(other)] = overload.parameter_types() else {
                            continue;
                        };
                        let Type::KnownInstance(KnownInstanceType::ConstraintSet(other)) = other
                        else {
                            continue;
                        };

                        let result = tracked
                            .constraints(db)
                            .implies(db, || other.constraints(db));
                        let tracked = TrackedConstraintSet::new(db, result);
                        overload.set_return_type(Type::KnownInstance(
                            KnownInstanceType::ConstraintSet(tracked),
                        ));
                    }

                    Type::KnownBoundMethod(
                        KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(tracked),
                    ) => {
                        let extract_inferable = |instance: &NominalInstanceType<'db>| {
                            if instance.has_known_class(db, KnownClass::NoneType) {
                                // Caller explicitly passed None, so no typevars are inferable.
                                return Some(FxHashSet::default());
                            }
                            instance
                                .tuple_spec(db)?
                                .fixed_elements()
                                .map(|ty| {
                                    ty.as_typevar()
                                        .map(|bound_typevar| bound_typevar.identity(db))
                                })
                                .collect()
                        };

                        let inferable = match overload.parameter_types() {
                            // Caller did not provide argument, so no typevars are inferable.
                            [None] => FxHashSet::default(),
                            [Some(Type::NominalInstance(instance))] => {
                                match extract_inferable(instance) {
                                    Some(inferable) => inferable,
                                    None => continue,
                                }
                            }
                            _ => continue,
                        };

                        let result = tracked
                            .constraints(db)
                            .satisfied_by_all_typevars(db, InferableTypeVars::One(&inferable));
                        overload.set_return_type(Type::BooleanLiteral(result));
                    }

                    Type::KnownBoundMethod(
                        KnownBoundMethodType::GenericContextSpecializeConstrained(generic_context),
                    ) => {
                        let [Some(constraints)] = overload.parameter_types() else {
                            continue;
                        };
                        let Type::KnownInstance(KnownInstanceType::ConstraintSet(constraints)) =
                            constraints
                        else {
                            continue;
                        };
                        let specialization =
                            generic_context.specialize_constrained(db, constraints.constraints(db));
                        let result = match specialization {
                            Ok(specialization) => Type::KnownInstance(
                                KnownInstanceType::Specialization(specialization),
                            ),
                            Err(()) => Type::none(db),
                        };
                        overload.set_return_type(result);
                    }

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
                                overload.set_return_type(arg.dunder_class(db));
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
                                // We deliberately use `.iterate()` here (falling back to `Unknown` if it isn't iterable)
                                // rather than `.try_iterate().expect()`. Even though we know at this point that the input
                                // type is assignable to `Iterable`, that doesn't mean that the input type is *actually*
                                // iterable (it could be a Liskov-uncompliant subtype of the `Iterable` class that sets
                                // `__iter__ = None`, for example). That would be badly written Python code, but we still
                                // need to be able to handle it without crashing.
                                let return_type = if let Type::Union(union) = argument {
                                    union.map(db, |element| {
                                        Type::tuple(TupleType::new(db, &element.iterate(db)))
                                    })
                                } else {
                                    Type::tuple(TupleType::new(db, &argument.iterate(db)))
                                };
                                overload.set_return_type(return_type);
                            }
                        }

                        _ => {}
                    },

                    Type::SpecialForm(SpecialFormType::TypedDict) => {
                        overload.set_return_type(todo_type!("Support for functional `TypedDict`"));
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
        self.iter()
    }
}

impl<'db> IntoIterator for Bindings<'db> {
    type Item = CallableBinding<'db>;
    type IntoIter = smallvec::IntoIter<[CallableBinding<'db>; 1]>;

    fn into_iter(self) -> Self::IntoIter {
        self.elements.into_iter()
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
            elements: smallvec_inline![from],
            argument_forms: ArgumentForms::new(0),
            constructor_instance_type: None,
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
            constructor_instance_type: None,
            overload_call_return_type: None,
            matching_overload_before_type_checking: None,
            overloads: smallvec_inline![from],
        };
        Bindings {
            callable_type,
            elements: smallvec_inline![callable_binding],
            argument_forms: ArgumentForms::new(0),
            constructor_instance_type: None,
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
#[derive(Debug, Clone)]
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

    /// The type of the instance being constructed, if this signature is for a constructor.
    pub(crate) constructor_instance_type: Option<Type<'db>>,

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

    /// The index of the overload that matched for this overloaded callable before type checking.
    ///
    /// This is [`Some`] only for step 1 of the [overload call evaluation algorithm][1] to surface
    /// the diagnostics for the matching overload directly instead of using the
    /// `no-matching-overload` diagnostic. The [`Self::matching_overload_index`] method cannot be
    /// used here because a single overload could be matched in step 1 but then filtered out in the
    /// following steps.
    ///
    /// [1]: https://typing.python.org/en/latest/spec/overload.html#overload-call-evaluation
    matching_overload_before_type_checking: Option<usize>,

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
            constructor_instance_type: None,
            overload_call_return_type: None,
            matching_overload_before_type_checking: None,
            overloads,
        }
    }

    pub(crate) fn not_callable(signature_type: Type<'db>) -> Self {
        Self {
            callable_type: signature_type,
            signature_type,
            dunder_call_is_possibly_unbound: false,
            bound_type: None,
            constructor_instance_type: None,
            overload_call_return_type: None,
            matching_overload_before_type_checking: None,
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
        db: &'db dyn Db,
        arguments: &CallArguments<'_, 'db>,
        argument_forms: &mut ArgumentForms,
    ) {
        // If this callable is a bound method, prepend the self instance onto the arguments list
        // before checking.
        let arguments = arguments.with_self(self.bound_type);

        for overload in &mut self.overloads {
            overload.match_parameters(db, arguments.as_ref(), argument_forms);
        }
    }

    fn check_types(
        &mut self,
        db: &'db dyn Db,
        argument_types: &CallArguments<'_, 'db>,
        call_expression_tcx: TypeContext<'db>,
    ) -> Option<ArgumentForms> {
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
                    overload.check_types(db, argument_types.as_ref(), call_expression_tcx);
                }
                return None;
            }
            MatchingOverloadIndex::Single(index) => {
                // If only one candidate overload remains, it is the winning match. Evaluate it as
                // a regular (non-overloaded) call.
                self.matching_overload_before_type_checking = Some(index);
                self.overloads[index].check_types(db, argument_types.as_ref(), call_expression_tcx);
                return None;
            }
            MatchingOverloadIndex::Multiple(indexes) => {
                // If two or more candidate overloads remain, proceed to step 2.
                indexes
            }
        };

        // Step 2: Evaluate each remaining overload as a regular (non-overloaded) call to determine
        // whether it is compatible with the supplied argument list.
        for (_, overload) in self.matching_overloads_mut() {
            overload.check_types(db, argument_types.as_ref(), call_expression_tcx);
        }

        match self.matching_overload_index() {
            MatchingOverloadIndex::None => {
                // If all overloads result in errors, proceed to step 3.
            }
            MatchingOverloadIndex::Single(_) => {
                // If only one overload evaluates without error, it is the winning match.
                return None;
            }
            MatchingOverloadIndex::Multiple(indexes) => {
                // If two or more candidate overloads remain, proceed to step 4.
                self.filter_overloads_containing_variadic(&indexes);

                match self.matching_overload_index() {
                    MatchingOverloadIndex::None => {
                        // This shouldn't be possible because step 4 can only filter out overloads
                        // when there _is_ a matching variadic argument.
                        tracing::debug!("All overloads have been filtered out in step 4");
                        return None;
                    }
                    MatchingOverloadIndex::Single(_) => {
                        // If only one candidate overload remains, it is the winning match.
                        return None;
                    }
                    MatchingOverloadIndex::Multiple(indexes) => {
                        // If two or more candidate overloads remain, proceed to step 5.
                        self.filter_overloads_using_any_or_unknown(
                            db,
                            argument_types.as_ref(),
                            &indexes,
                        );
                    }
                }

                // This shouldn't lead to argument type expansion.
                return None;
            }
        }

        // Step 3: Perform "argument type expansion". Reference:
        // https://typing.python.org/en/latest/spec/overload.html#argument-type-expansion
        let mut expansions = argument_types.expand(db).peekable();

        // Return early if there are no argument types to expand.
        expansions.peek()?;

        // At this point, there's at least one argument that can be expanded.
        //
        // This heuristic tries to detect if there's any need to perform argument type expansion or
        // not by checking whether there are any non-expandable argument type that cannot be
        // assigned to any of the overloads.
        for (argument_index, (argument, argument_type)) in argument_types.iter().enumerate() {
            // TODO: Remove `Keywords` once `**kwargs` support is added
            if matches!(argument, Argument::Synthetic | Argument::Keywords) {
                continue;
            }
            let Some(argument_type) = argument_type else {
                continue;
            };
            if is_expandable_type(db, argument_type) {
                continue;
            }
            let mut is_argument_assignable_to_any_overload = false;
            'overload: for overload in &self.overloads {
                for parameter_index in &overload.argument_matches[argument_index].parameters {
                    let parameter_type = overload.signature.parameters()[*parameter_index]
                        .annotated_type()
                        .unwrap_or(Type::unknown());
                    if argument_type
                        .when_assignable_to(db, parameter_type, overload.inferable_typevars)
                        .is_always_satisfied(db)
                    {
                        is_argument_assignable_to_any_overload = true;
                        break 'overload;
                    }
                }
            }
            if !is_argument_assignable_to_any_overload {
                tracing::debug!(
                    "Argument at {argument_index} (`{}`) is not assignable to any of the \
                    remaining overloads, skipping argument type expansion",
                    argument_type.display(db)
                );
                return None;
            }
        }

        let snapshotter = CallableBindingSnapshotter::new(matching_overload_indexes);

        // State of the bindings _after_ evaluating (type checking) the matching overloads using
        // the non-expanded argument types.
        let post_evaluation_snapshot = snapshotter.take(self);

        for expansion in expansions {
            let expanded_argument_lists = match expansion {
                Expansion::LimitReached(index) => {
                    snapshotter.restore(self, post_evaluation_snapshot);
                    self.overload_call_return_type = Some(
                        OverloadCallReturnType::ArgumentTypeExpansionLimitReached(index),
                    );
                    return None;
                }
                Expansion::Expanded(argument_lists) => argument_lists,
            };

            // This is the merged state of the bindings after evaluating all of the expanded
            // argument lists. This will be the final state to restore the bindings to if all of
            // the expanded argument lists evaluated successfully.
            let mut merged_evaluation_state: Option<CallableBindingSnapshot<'db>> = None;

            // Merged argument forms after evaluating all the argument lists in this expansion.
            let mut merged_argument_forms = ArgumentForms::default();

            // The return types of each of the expanded argument lists that evaluated successfully.
            let mut return_types = Vec::new();

            for expanded_arguments in &expanded_argument_lists {
                let mut argument_forms = ArgumentForms::new(expanded_arguments.len());

                // The spec mentions that each expanded argument list should be re-evaluated from
                // step 2 but we need to re-evaluate from step 1 because our step 1 does more than
                // what the spec mentions. Step 1 of the spec means only "eliminate impossible
                // overloads due to arity mismatch" while our step 1 (`match_parameters`) also
                // includes "match arguments to the parameters". This is important because it
                // allows us to correctly handle cases involving a variadic argument that could
                // expand into different number of arguments with each expansion. Refer to
                // https://github.com/astral-sh/ty/issues/735 for more details.
                for overload in &mut self.overloads {
                    // Clear the state of all overloads before re-evaluating from step 1
                    overload.reset();
                    overload.match_parameters(db, expanded_arguments, &mut argument_forms);
                }

                merged_argument_forms.merge(&argument_forms);

                for (_, overload) in self.matching_overloads_mut() {
                    overload.check_types(db, expanded_arguments, call_expression_tcx);
                }

                let return_type = match self.matching_overload_index() {
                    MatchingOverloadIndex::None => None,
                    MatchingOverloadIndex::Single(index) => {
                        Some(self.overloads[index].return_type())
                    }
                    MatchingOverloadIndex::Multiple(matching_overload_indexes) => {
                        self.filter_overloads_containing_variadic(&matching_overload_indexes);

                        match self.matching_overload_index() {
                            MatchingOverloadIndex::None => {
                                tracing::debug!(
                                    "All overloads have been filtered out in step 4 during argument type expansion"
                                );
                                None
                            }
                            MatchingOverloadIndex::Single(_) => Some(self.return_type()),
                            MatchingOverloadIndex::Multiple(indexes) => {
                                self.filter_overloads_using_any_or_unknown(
                                    db,
                                    expanded_arguments,
                                    &indexes,
                                );
                                Some(self.return_type())
                            }
                        }
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

                return Some(merged_argument_forms);
            }
        }

        // If the type expansion didn't yield any successful return type, we need to restore the
        // bindings state back to the one after the type checking step using the non-expanded
        // argument types. This is necessary because we restore the state to the pre-evaluation
        // snapshot when processing the expanded argument lists.
        snapshotter.restore(self, post_evaluation_snapshot);

        None
    }

    /// Filter overloads based on variadic argument to variadic parameter match.
    ///
    /// This is the step 4 of the [overload call evaluation algorithm][1].
    ///
    /// [1]: https://typing.python.org/en/latest/spec/overload.html#overload-call-evaluation
    fn filter_overloads_containing_variadic(&mut self, matching_overload_indexes: &[usize]) {
        let variadic_matching_overloads = matching_overload_indexes
            .iter()
            .filter(|&&overload_index| {
                self.overloads[overload_index].variadic_argument_matched_to_variadic_parameter
            })
            .collect::<HashSet<_>>();

        if variadic_matching_overloads.is_empty()
            || variadic_matching_overloads.len() == matching_overload_indexes.len()
        {
            return;
        }

        for overload_index in matching_overload_indexes {
            if !variadic_matching_overloads.contains(overload_index) {
                self.overloads[*overload_index].mark_as_unmatched_overload();
            }
        }
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
        // The maximum number of parameters across all the overloads that are being considered
        // for filtering.
        let max_parameter_count = matching_overload_indexes
            .iter()
            .map(|&index| self.overloads[index].signature.parameters().len())
            .max()
            .unwrap_or(0);

        // These are the parameter indexes that matches the arguments that participate in the
        // filtering process.
        //
        // The parameter types at these indexes have at least one overload where the type isn't
        // gradual equivalent to the parameter types at the same index for other overloads.
        let mut participating_parameter_indexes = HashSet::new();

        // The parameter types at each index for the first overload containing a parameter at
        // that index.
        let mut first_parameter_types: Vec<Option<Type<'db>>> = vec![None; max_parameter_count];

        for argument_index in 0..arguments.len() {
            for overload_index in matching_overload_indexes {
                let overload = &self.overloads[*overload_index];
                for &parameter_index in &overload.argument_matches[argument_index].parameters {
                    // TODO: For an unannotated `self` / `cls` parameter, the type should be
                    // `typing.Self` / `type[typing.Self]`
                    let current_parameter_type = overload.signature.parameters()[parameter_index]
                        .annotated_type()
                        .unwrap_or(Type::unknown());
                    let first_parameter_type = &mut first_parameter_types[parameter_index];
                    if let Some(first_parameter_type) = first_parameter_type {
                        if !first_parameter_type
                            .when_equivalent_to(
                                db,
                                current_parameter_type,
                                overload.inferable_typevars,
                            )
                            .is_always_satisfied(db)
                        {
                            participating_parameter_indexes.insert(parameter_index);
                        }
                    } else {
                        *first_parameter_type = Some(current_parameter_type);
                    }
                }
            }
        }

        let mut union_argument_type_builders = std::iter::repeat_with(|| UnionBuilder::new(db))
            .take(max_parameter_count)
            .collect::<Vec<_>>();

        for (argument_index, argument_type) in arguments.iter_types().enumerate() {
            for overload_index in matching_overload_indexes {
                let overload = &self.overloads[*overload_index];
                for (parameter_index, variadic_argument_type) in
                    overload.argument_matches[argument_index].iter()
                {
                    if !participating_parameter_indexes.contains(&parameter_index) {
                        continue;
                    }
                    union_argument_type_builders[parameter_index].add_in_place(
                        variadic_argument_type
                            .unwrap_or(argument_type)
                            .top_materialization(db),
                    );
                }
            }
        }

        // These only contain the top materialized argument types for the corresponding
        // participating parameter indexes.
        let top_materialized_argument_type = Type::heterogeneous_tuple(
            db,
            union_argument_type_builders
                .into_iter()
                .filter_map(|builder| {
                    if builder.is_empty() {
                        None
                    } else {
                        Some(builder.build())
                    }
                }),
        );

        // A flag to indicate whether we've found the overload that makes the remaining overloads
        // unmatched for the given argument types.
        let mut filter_remaining_overloads = false;

        for (upto, current_index) in matching_overload_indexes.iter().enumerate() {
            if filter_remaining_overloads {
                self.overloads[*current_index].mark_as_unmatched_overload();
                continue;
            }

            let mut union_parameter_types = std::iter::repeat_with(|| UnionBuilder::new(db))
                .take(max_parameter_count)
                .collect::<Vec<_>>();

            // The number of parameters that have been skipped because they don't participate in
            // the filtering process. This is used to make sure the types are added to the
            // corresponding parameter index in `union_parameter_types`.
            let mut skipped_parameters = 0;

            for argument_index in 0..arguments.len() {
                for overload_index in &matching_overload_indexes[..=upto] {
                    let overload = &self.overloads[*overload_index];
                    for parameter_index in &overload.argument_matches[argument_index].parameters {
                        if !participating_parameter_indexes.contains(parameter_index) {
                            skipped_parameters += 1;
                            continue;
                        }
                        // TODO: For an unannotated `self` / `cls` parameter, the type should be
                        // `typing.Self` / `type[typing.Self]`
                        let mut parameter_type = overload.signature.parameters()[*parameter_index]
                            .annotated_type()
                            .unwrap_or(Type::unknown());
                        if let Some(specialization) = overload.specialization {
                            parameter_type =
                                parameter_type.apply_specialization(db, specialization);
                        }
                        union_parameter_types[parameter_index.saturating_sub(skipped_parameters)]
                            .add_in_place(parameter_type);
                    }
                }
            }

            let parameter_types = Type::heterogeneous_tuple(
                db,
                union_parameter_types.into_iter().filter_map(|builder| {
                    if builder.is_empty() {
                        None
                    } else {
                        Some(builder.build())
                    }
                }),
            );

            if top_materialized_argument_type.is_assignable_to(db, parameter_types) {
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
                        .when_equivalent_to(
                            db,
                            first_overload_return_type,
                            overload.inferable_typevars,
                        )
                        .is_always_satisfied(db)
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
    pub(crate) fn matching_overload_index(&self) -> MatchingOverloadIndex {
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

    /// Returns all overloads for this call binding, including overloads that did not match.
    pub(crate) fn overloads(&self) -> &[Binding<'db>] {
        self.overloads.as_slice()
    }

    /// Returns an iterator over all the overloads that matched for this call binding.
    pub(crate) fn matching_overloads(
        &self,
    ) -> impl Iterator<Item = (usize, &Binding<'db>)> + Clone {
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
                OverloadCallReturnType::ArgumentTypeExpansionLimitReached(_)
                | OverloadCallReturnType::Ambiguous => Type::unknown(),
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
                    "Object of type `{}` is not callable (possibly missing `__call__` method)",
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
                    Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(
                        function,
                    )) => Some((FunctionKind::MethodWrapper, function)),
                    _ => None,
                };

                // If there is a single matching overload, the diagnostics should be reported
                // directly for that overload.
                if let Some(matching_overload_index) = self.matching_overload_before_type_checking {
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

                if let Some(index) =
                    self.overload_call_return_type
                        .and_then(
                            |overload_call_return_type| match overload_call_return_type {
                                OverloadCallReturnType::ArgumentTypeExpansionLimitReached(
                                    index,
                                ) => Some(index),
                                _ => None,
                            },
                        )
                {
                    diag.info(format_args!(
                        "Limit of argument type expansion reached at argument {index}"
                    ));
                }

                if let Some((kind, function)) = function_type_and_kind {
                    let (overloads, implementation) =
                        function.overloads_and_implementation(context.db());

                    if let Some(spans) = overloads
                        .first()
                        .and_then(|overload| overload.spans(context.db()))
                    {
                        let mut sub = SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            "First overload defined here",
                        );
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
                            overload.signature(context.db()).display(context.db())
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
                            SubDiagnosticSeverity::Info,
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

impl<'db> IntoIterator for CallableBinding<'db> {
    type Item = Binding<'db>;
    type IntoIter = smallvec::IntoIter<[Binding<'db>; 1]>;

    fn into_iter(self) -> Self::IntoIter {
        self.overloads.into_iter()
    }
}

#[derive(Debug, Copy, Clone)]
enum OverloadCallReturnType<'db> {
    ArgumentTypeExpansion(Type<'db>),
    ArgumentTypeExpansionLimitReached(usize),
    Ambiguous,
}

#[derive(Debug)]
pub(crate) enum MatchingOverloadIndex {
    /// No matching overloads found.
    None,

    /// Exactly one matching overload found at the given index.
    Single(usize),

    /// Multiple matching overloads found at the given indexes.
    Multiple(Vec<usize>),
}

#[derive(Default, Debug, Clone)]
struct ArgumentForms {
    values: Vec<Option<ParameterForm>>,
    conflicting: Vec<bool>,
}

impl ArgumentForms {
    /// Create a new argument forms initialized to the given length and the default values.
    fn new(len: usize) -> Self {
        Self {
            values: vec![None; len],
            conflicting: vec![false; len],
        }
    }

    fn merge(&mut self, other: &ArgumentForms) {
        if self.values.len() < other.values.len() {
            self.values.resize(other.values.len(), None);
            self.conflicting.resize(other.conflicting.len(), false);
        }

        for (index, (other_form, other_conflict)) in other
            .values
            .iter()
            .zip(other.conflicting.iter())
            .enumerate()
        {
            if let Some(self_form) = &mut self.values[index] {
                if let Some(other_form) = other_form {
                    if *self_form != *other_form {
                        // Different parameter forms, mark as conflicting
                        self.conflicting[index] = true;
                        *self_form = *other_form; // Use the new form
                    }
                }
            } else {
                self.values[index] = *other_form;
            }

            // Update the conflicting form (true takes precedence)
            self.conflicting[index] |= *other_conflict;
        }
    }

    fn shrink_to_fit(&mut self) {
        self.values.shrink_to_fit();
        self.conflicting.shrink_to_fit();
    }
}

#[derive(Default, Clone, Copy)]
struct ParameterInfo {
    matched: bool,
    suppress_missing_error: bool,
}

struct ArgumentMatcher<'a, 'db> {
    parameters: &'a Parameters<'db>,
    argument_forms: &'a mut ArgumentForms,
    errors: &'a mut Vec<BindingError<'db>>,

    argument_matches: Vec<MatchedArgument<'db>>,
    parameter_info: Vec<ParameterInfo>,
    next_positional: usize,
    first_excess_positional: Option<usize>,
    num_synthetic_args: usize,
    variadic_argument_matched_to_variadic_parameter: bool,
}

impl<'a, 'db> ArgumentMatcher<'a, 'db> {
    fn new(
        arguments: &CallArguments,
        parameters: &'a Parameters<'db>,
        argument_forms: &'a mut ArgumentForms,
        errors: &'a mut Vec<BindingError<'db>>,
    ) -> Self {
        Self {
            parameters,
            argument_forms,
            errors,
            argument_matches: vec![MatchedArgument::default(); arguments.len()],
            parameter_info: vec![ParameterInfo::default(); parameters.len()],
            next_positional: 0,
            first_excess_positional: None,
            num_synthetic_args: 0,
            variadic_argument_matched_to_variadic_parameter: false,
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

    #[expect(clippy::too_many_arguments)]
    fn assign_argument(
        &mut self,
        argument_index: usize,
        argument: Argument<'a>,
        argument_type: Option<Type<'db>>,
        parameter_index: usize,
        parameter: &Parameter<'db>,
        positional: bool,
        variable_argument_length: bool,
    ) {
        if !matches!(argument, Argument::Synthetic) {
            let adjusted_argument_index = argument_index - self.num_synthetic_args;
            if let Some(existing) =
                self.argument_forms.values[adjusted_argument_index].replace(parameter.form)
            {
                if existing != parameter.form {
                    self.argument_forms.conflicting[argument_index - self.num_synthetic_args] =
                        true;
                }
            }
        }
        if self.parameter_info[parameter_index].matched {
            if !parameter.is_variadic() && !parameter.is_keyword_variadic() {
                self.errors.push(BindingError::ParameterAlreadyAssigned {
                    argument_index: self.get_argument_index(argument_index),
                    parameter: ParameterContext::new(parameter, parameter_index, positional),
                });
            }
        }
        if variable_argument_length
            && matches!(
                (argument, parameter.kind()),
                (Argument::Variadic, ParameterKind::Variadic { .. })
                    | (Argument::Keywords, ParameterKind::KeywordVariadic { .. })
            )
        {
            self.variadic_argument_matched_to_variadic_parameter = true;
        }
        let matched_argument = &mut self.argument_matches[argument_index];
        matched_argument.parameters.push(parameter_index);
        matched_argument.types.push(argument_type);
        matched_argument.matched = true;
        self.parameter_info[parameter_index].matched = true;
    }

    fn match_positional(
        &mut self,
        argument_index: usize,
        argument: Argument<'a>,
        argument_type: Option<Type<'db>>,
        variable_argument_length: bool,
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
            argument_type,
            parameter_index,
            parameter,
            !parameter.is_variadic(),
            variable_argument_length,
        );
        Ok(())
    }

    fn match_keyword(
        &mut self,
        argument_index: usize,
        argument: Argument<'a>,
        argument_type: Option<Type<'db>>,
        name: &str,
    ) -> Result<(), ()> {
        let Some((parameter_index, parameter)) = self
            .parameters
            .keyword_by_name(name)
            .or_else(|| self.parameters.keyword_variadic())
        else {
            if let Some((parameter_index, parameter)) =
                self.parameters.positional_only_by_name(name)
            {
                self.errors
                    .push(BindingError::PositionalOnlyParameterAsKwarg {
                        argument_index: self.get_argument_index(argument_index),
                        parameter: ParameterContext::new(parameter, parameter_index, true),
                    });
                self.parameter_info[parameter_index].suppress_missing_error = true;
            } else {
                self.errors.push(BindingError::UnknownArgument {
                    argument_name: ast::name::Name::new(name),
                    argument_index: self.get_argument_index(argument_index),
                });
            }
            return Err(());
        };
        self.assign_argument(
            argument_index,
            argument,
            argument_type,
            parameter_index,
            parameter,
            false,
            false,
        );
        Ok(())
    }

    /// Match a variadic argument to the remaining positional, standard or variadic parameters.
    fn match_variadic(
        &mut self,
        db: &'db dyn Db,
        argument_index: usize,
        argument: Argument<'a>,
        argument_type: Option<Type<'db>>,
    ) -> Result<(), ()> {
        enum VariadicArgumentType<'db> {
            ParamSpec(Type<'db>),
            Other(Cow<'db, TupleSpec<'db>>),
            None,
        }

        let variadic_type = match argument_type {
            Some(argument_type @ Type::Union(union)) => {
                // When accessing an instance attribute that is a `P.args`, the type we infer is
                // `Unknown | P.args`. This needs to be special cased here to avoid calling
                // `iterate` on it which will lose the `ParamSpec` information as it will return
                // `object` that comes from the upper bound of `P.args`. What we want is to always
                // use the `P.args` type to perform type checking against the parameter type. This
                // will allow us to error when `*args: P.args` is matched against, for example,
                // `n: int` and correctly type check when `*args: P.args` is matched against
                // `*args: P.args` (another ParamSpec).
                match union.elements(db) {
                    [paramspec @ Type::TypeVar(typevar), other]
                    | [other, paramspec @ Type::TypeVar(typevar)]
                        if typevar.is_paramspec(db) && other.is_unknown() =>
                    {
                        VariadicArgumentType::ParamSpec(*paramspec)
                    }
                    _ => {
                        // TODO: Same todo comment as in the non-paramspec case below
                        VariadicArgumentType::Other(argument_type.iterate(db))
                    }
                }
            }
            Some(paramspec @ Type::TypeVar(typevar)) if typevar.is_paramspec(db) => {
                VariadicArgumentType::ParamSpec(paramspec)
            }
            Some(argument_type) => {
                // TODO: `Type::iterate` internally handles unions, but in a lossy way.
                // It might be superior here to manually map over the union and call `try_iterate`
                // on each element, similar to the way that `unpacker.rs` does in the `unpack_inner` method.
                // It might be a bit of a refactor, though.
                // See <https://github.com/astral-sh/ruff/pull/20377#issuecomment-3401380305>
                // for more details. --Alex
                VariadicArgumentType::Other(argument_type.iterate(db))
            }
            None => VariadicArgumentType::None,
        };

        let (mut argument_types, length, variable_element) = match &variadic_type {
            VariadicArgumentType::ParamSpec(paramspec) => (
                Either::Right(std::iter::empty()),
                TupleLength::unknown(),
                Some(*paramspec),
            ),
            VariadicArgumentType::Other(tuple) => (
                Either::Left(tuple.all_elements().copied()),
                tuple.len(),
                tuple.variable_element().copied(),
            ),
            VariadicArgumentType::None => (
                Either::Right(std::iter::empty()),
                TupleLength::unknown(),
                None,
            ),
        };

        let is_variable = length.is_variable();

        // We must be able to match up the fixed-length portion of the argument with positional
        // parameters, so we pass on any errors that occur.
        for _ in 0..length.minimum() {
            self.match_positional(
                argument_index,
                argument,
                argument_types.next().or(variable_element),
                is_variable,
            )?;
        }

        // If the tuple is variable-length, we assume that it will soak up all remaining positional
        // parameters.
        if is_variable {
            while self
                .parameters
                .get_positional(self.next_positional)
                .is_some()
            {
                self.match_positional(
                    argument_index,
                    argument,
                    argument_types.next().or(variable_element),
                    is_variable,
                )?;
            }
        }

        // Finally, if there is a variadic parameter we can match any of the remaining unpacked
        // argument types to it, but only if there is at least one remaining argument type. This is
        // because a variadic parameter is optional, so if this was done unconditionally, ty could
        // raise a false positive as "too many arguments".
        if self.parameters.variadic().is_some() {
            if let Some(argument_type) = argument_types.next().or(variable_element) {
                self.match_positional(argument_index, argument, Some(argument_type), is_variable)?;
                for argument_type in argument_types {
                    self.match_positional(
                        argument_index,
                        argument,
                        Some(argument_type),
                        is_variable,
                    )?;
                }
            }
        }

        Ok(())
    }

    fn match_keyword_variadic(
        &mut self,
        db: &'db dyn Db,
        argument_index: usize,
        argument_type: Option<Type<'db>>,
    ) {
        if let Some(Type::TypedDict(typed_dict)) = argument_type {
            // Special case TypedDict because we know which keys are present.
            for (name, field) in typed_dict.items(db) {
                let _ = self.match_keyword(
                    argument_index,
                    Argument::Keywords,
                    Some(field.declared_ty),
                    name.as_str(),
                );
            }
        } else {
            let dunder_getitem_return_type = |ty: Type<'db>| match ty
                .member_lookup_with_policy(
                    db,
                    Name::new_static("__getitem__"),
                    MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                )
                .place
            {
                Place::Defined(getitem_method, _, Definedness::AlwaysDefined) => getitem_method
                    .try_call(db, &CallArguments::positional([Type::unknown()]))
                    .ok()
                    .map_or_else(Type::unknown, |bindings| bindings.return_type(db)),
                _ => Type::unknown(),
            };

            let value_type = match argument_type {
                Some(argument_type @ Type::Union(union)) => {
                    // See the comment in `match_variadic` for why we special case this situation.
                    match union.elements(db) {
                        [paramspec @ Type::TypeVar(typevar), other]
                        | [other, paramspec @ Type::TypeVar(typevar)]
                            if typevar.is_paramspec(db) && other.is_unknown() =>
                        {
                            *paramspec
                        }
                        _ => dunder_getitem_return_type(argument_type),
                    }
                }
                Some(paramspec @ Type::TypeVar(typevar)) if typevar.is_paramspec(db) => paramspec,
                Some(argument_type) => dunder_getitem_return_type(argument_type),
                None => Type::unknown(),
            };

            for (parameter_index, parameter) in self.parameters.iter().enumerate() {
                if self.parameter_info[parameter_index].matched && !parameter.is_keyword_variadic()
                {
                    continue;
                }
                if matches!(
                    parameter.kind(),
                    ParameterKind::PositionalOnly { .. } | ParameterKind::Variadic { .. }
                ) {
                    continue;
                }
                self.assign_argument(
                    argument_index,
                    Argument::Keywords,
                    Some(value_type),
                    parameter_index,
                    parameter,
                    false,
                    true,
                );
            }
        }
    }

    fn finish(self) -> Box<[MatchedArgument<'db>]> {
        if let Some(first_excess_argument_index) = self.first_excess_positional {
            self.errors.push(BindingError::TooManyPositionalArguments {
                first_excess_argument_index: self.get_argument_index(first_excess_argument_index),
                expected_positional_count: self.parameters.positional().count(),
                provided_positional_count: self.next_positional,
            });
        }

        let mut missing = vec![];
        for (
            index,
            ParameterInfo {
                matched,
                suppress_missing_error,
            },
        ) in self.parameter_info.iter().copied().enumerate()
        {
            if !matched {
                if suppress_missing_error {
                    continue;
                }
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

        self.argument_matches.into_boxed_slice()
    }
}

struct ArgumentTypeChecker<'a, 'db> {
    db: &'db dyn Db,
    signature_type: Type<'db>,
    signature: &'a Signature<'db>,
    arguments: &'a CallArguments<'a, 'db>,
    argument_matches: &'a [MatchedArgument<'db>],
    parameter_tys: &'a mut [Option<Type<'db>>],
    constructor_instance_type: Option<Type<'db>>,
    call_expression_tcx: TypeContext<'db>,
    return_ty: Type<'db>,
    errors: &'a mut Vec<BindingError<'db>>,

    inferable_typevars: InferableTypeVars<'db, 'db>,
    specialization: Option<Specialization<'db>>,
}

impl<'a, 'db> ArgumentTypeChecker<'a, 'db> {
    #[expect(clippy::too_many_arguments)]
    fn new(
        db: &'db dyn Db,
        signature_type: Type<'db>,
        signature: &'a Signature<'db>,
        arguments: &'a CallArguments<'a, 'db>,
        argument_matches: &'a [MatchedArgument<'db>],
        parameter_tys: &'a mut [Option<Type<'db>>],
        constructor_instance_type: Option<Type<'db>>,
        call_expression_tcx: TypeContext<'db>,
        return_ty: Type<'db>,
        errors: &'a mut Vec<BindingError<'db>>,
    ) -> Self {
        Self {
            db,
            signature_type,
            signature,
            arguments,
            argument_matches,
            parameter_tys,
            constructor_instance_type,
            call_expression_tcx,
            return_ty,
            errors,
            inferable_typevars: InferableTypeVars::None,
            specialization: None,
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
        let Some(generic_context) = self.signature.generic_context else {
            return;
        };

        let return_with_tcx = self
            .constructor_instance_type
            .or(self.signature.return_ty)
            .zip(self.call_expression_tcx.annotation);

        self.inferable_typevars = generic_context.inferable_typevars(self.db);
        let mut builder = SpecializationBuilder::new(self.db, self.inferable_typevars);

        // Prefer the declared type of generic classes.
        let preferred_type_mappings = return_with_tcx.and_then(|(return_ty, tcx)| {
            tcx.filter_union(self.db, |ty| ty.class_specialization(self.db).is_some())
                .class_specialization(self.db)?;

            builder.infer(return_ty, tcx).ok()?;
            Some(builder.type_mappings().clone())
        });

        // For a given type variable, we track the variance of any assignments to that type variable
        // in the argument types.
        let mut variance_in_arguments: FxHashMap<BoundTypeVarIdentity<'_>, TypeVarVariance> =
            FxHashMap::default();

        let parameters = self.signature.parameters();
        for (argument_index, adjusted_argument_index, _, argument_type) in
            self.enumerate_argument_types()
        {
            for (parameter_index, variadic_argument_type) in
                self.argument_matches[argument_index].iter()
            {
                let parameter = &parameters[parameter_index];
                let Some(expected_type) = parameter.annotated_type() else {
                    continue;
                };

                let specialization_result = builder.infer_map(
                    expected_type,
                    variadic_argument_type.unwrap_or(argument_type),
                    |(identity, variance, inferred_ty)| {
                        // Avoid widening the inferred type if it is already assignable to the
                        // preferred declared type.
                        if preferred_type_mappings
                            .as_ref()
                            .and_then(|types| types.get(&identity))
                            .is_some_and(|preferred_ty| {
                                inferred_ty.is_assignable_to(self.db, *preferred_ty)
                            })
                        {
                            return None;
                        }

                        variance_in_arguments
                            .entry(identity)
                            .and_modify(|current| *current = current.join(variance))
                            .or_insert(variance);

                        Some(inferred_ty)
                    },
                );

                if let Err(error) = specialization_result {
                    self.errors.push(BindingError::SpecializationError {
                        error,
                        argument_index: adjusted_argument_index,
                    });
                }
            }
        }

        // Attempt to promote any literal types assigned to the specialization.
        let maybe_promote = |identity, typevar, ty: Type<'db>| {
            let Some(return_ty) = self.constructor_instance_type.or(self.signature.return_ty)
            else {
                return ty;
            };

            let mut combined_tcx = TypeContext::default();
            let mut variance_in_return = TypeVarVariance::Bivariant;

            // Find all occurrences of the type variable in the return type.
            let visit_return_ty = |_, ty, variance, tcx: TypeContext<'db>| {
                if ty != Type::TypeVar(typevar) {
                    return;
                }

                // We always prefer the declared type when attempting literal promotion,
                // so we take the union of every applicable type context.
                match (tcx.annotation, &mut combined_tcx.annotation) {
                    (Some(_), None) => combined_tcx = tcx,
                    (Some(ty), Some(combined_ty)) => {
                        *combined_ty = UnionType::from_elements(self.db, [*combined_ty, ty]);
                    }
                    _ => {}
                }

                variance_in_return = variance_in_return.join(variance);
            };

            return_ty.visit_specialization(self.db, self.call_expression_tcx, visit_return_ty);

            // Promotion is only useful if the type variable is in invariant or contravariant
            // position in the return type.
            if variance_in_return.is_covariant() {
                return ty;
            }

            // If the type variable is a non-covariant position in the argument, then we avoid
            // promotion, respecting any literals in the parameter type.
            if variance_in_arguments
                .get(&identity)
                .is_some_and(|variance| !variance.is_covariant())
            {
                return ty;
            }

            ty.promote_literals(self.db, combined_tcx)
        };

        // Build the specialization first without inferring the complete type context.
        let isolated_specialization = builder
            .mapped(generic_context, maybe_promote)
            .build(generic_context);
        let isolated_return_ty = self
            .return_ty
            .apply_specialization(self.db, isolated_specialization);

        let mut try_infer_tcx = || {
            let (return_ty, call_expression_tcx) = return_with_tcx?;

            // A type variable is not a useful type-context for expression inference, and applying it
            // to the return type can lead to confusing unions in nested generic calls.
            if call_expression_tcx.is_type_var() {
                return None;
            }

            // If the return type is already assignable to the annotated type, we ignore the rest of
            // the type context and prefer the narrower inferred type.
            if isolated_return_ty.is_assignable_to(self.db, call_expression_tcx) {
                return None;
            }

            // TODO: Ideally we would infer the annotated type _before_ the arguments if this call is part of an
            // annotated assignment, to closer match the order of any unions written in the type annotation.
            builder.infer(return_ty, call_expression_tcx).ok()?;

            // Otherwise, build the specialization again after inferring the complete type context.
            let specialization = builder
                .mapped(generic_context, maybe_promote)
                .build(generic_context);
            let return_ty = return_ty.apply_specialization(self.db, specialization);

            Some((Some(specialization), return_ty))
        };

        (self.specialization, self.return_ty) =
            try_infer_tcx().unwrap_or((Some(isolated_specialization), isolated_return_ty));
    }

    fn check_argument_type(
        &mut self,
        adjusted_argument_index: Option<usize>,
        argument: Argument<'a>,
        mut argument_type: Type<'db>,
        parameter_index: usize,
    ) {
        let parameters = self.signature.parameters();
        let parameter = &parameters[parameter_index];
        if let Some(mut expected_ty) = parameter.annotated_type() {
            if let Some(specialization) = self.specialization {
                argument_type = argument_type.apply_specialization(self.db, specialization);
                expected_ty = expected_ty.apply_specialization(self.db, specialization);
            }
            // This is one of the few places where we want to check if there's _any_ specialization
            // where assignability holds; normally we want to check that assignability holds for
            // _all_ specializations.
            // TODO: Soon we will go further, and build the actual specializations from the
            // constraint set that we get from this assignability check, instead of inferring and
            // building them in an earlier separate step.
            if argument_type
                .when_assignable_to(self.db, expected_ty, self.inferable_typevars)
                .is_never_satisfied(self.db)
            {
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
        let paramspec = self
            .signature
            .parameters()
            .find_paramspec_from_args_kwargs(self.db);

        for (argument_index, adjusted_argument_index, argument, argument_type) in
            self.enumerate_argument_types()
        {
            if let Some((_, paramspec)) = paramspec {
                if self.try_paramspec_evaluation_at(argument_index, paramspec) {
                    // Once we find an argument that matches the `ParamSpec`, we can stop checking
                    // the remaining arguments since `ParamSpec` should always be the last
                    // parameter.
                    return;
                }
            }

            match argument {
                Argument::Variadic => self.check_variadic_argument_type(
                    argument_index,
                    adjusted_argument_index,
                    argument,
                ),
                Argument::Keywords => self.check_keyword_variadic_argument_type(
                    argument_index,
                    adjusted_argument_index,
                    argument,
                    argument_type,
                ),
                _ => {
                    // If the argument isn't splatted, just check its type directly.
                    for parameter_index in &self.argument_matches[argument_index].parameters {
                        self.check_argument_type(
                            adjusted_argument_index,
                            argument,
                            argument_type,
                            *parameter_index,
                        );
                    }
                }
            }
        }

        if let Some((_, paramspec)) = paramspec {
            // If we reach here, none of the arguments matched the `ParamSpec` parameter, but the
            // `ParamSpec` could specialize to a parameter list containing some parameters. For
            // example,
            //
            // ```py
            // from typing import Callable
            //
            // def foo[**P](f: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...
            //
            // def f(x: int) -> None: ...
            //
            // foo(f)
            // ```
            //
            // Here, no arguments match the `ParamSpec` parameter, but `P` specializes to `(x: int)`,
            // so we need to perform a sub-call with no arguments.
            self.evaluate_paramspec_sub_call(None, paramspec);
        }
    }

    /// Try to evaluate a `ParamSpec` sub-call at the given argument index.
    ///
    /// The `ParamSpec` parameter is always going to be at the end of the parameter list but there
    /// can be other parameter before it. If one of these prepended positional parameters contains
    /// a free `ParamSpec`, we consider that variable in scope for the purposes of extracting the
    /// components of that `ParamSpec`. For example:
    ///
    /// ```py
    /// from typing import Callable
    ///
    /// def foo[**P](f: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...
    ///
    /// def f(x: int, y: str) -> None: ...
    ///
    /// foo(f, 1, "hello")  # P: (x: int, y: str)
    /// ```
    ///
    /// Here, `P` specializes to `(x: int, y: str)` when `foo` is called with `f`, which means that
    /// the parameters of `f` become a part of `foo`'s parameter list replacing the `ParamSpec`
    /// parameter which is:
    ///
    /// ```py
    /// def foo(f: Callable[[x: int, y: str], None], x: int, y: str) -> None: ...
    /// ```
    ///
    /// This method will check whether the parameter matching the argument at `argument_index` is
    /// annotated with the components of `ParamSpec`, and if so, will invoke a sub-call considering
    /// the arguments starting from `argument_index` against the specialized parameter list.
    ///
    /// Returns `true` if the sub-call was invoked, `false` otherwise.
    fn try_paramspec_evaluation_at(
        &mut self,
        argument_index: usize,
        paramspec: BoundTypeVarInstance<'db>,
    ) -> bool {
        let [parameter_index] = self.argument_matches[argument_index].parameters.as_slice() else {
            return false;
        };

        if !self.signature.parameters()[*parameter_index]
            .annotated_type()
            .is_some_and(|ty| matches!(ty, Type::TypeVar(typevar) if typevar.is_paramspec(self.db)))
        {
            return false;
        }

        self.evaluate_paramspec_sub_call(Some(argument_index), paramspec)
    }

    /// Invoke a sub-call for the given `ParamSpec` type variable, using the remaining arguments.
    ///
    /// The remaining arguments start from `argument_index` if provided, otherwise no arguments
    /// are passed.
    ///
    /// This method returns `false` if the specialization does not contain a mapping for the given
    /// `paramspec`, contains an invalid mapping (i.e., not a `Callable` of kind `ParamSpecValue`)
    /// or if the value is an overloaded callable.
    ///
    /// For more details, refer to [`Self::try_paramspec_evaluation_at`].
    fn evaluate_paramspec_sub_call(
        &mut self,
        argument_index: Option<usize>,
        paramspec: BoundTypeVarInstance<'db>,
    ) -> bool {
        let Some(Type::Callable(callable)) = self
            .specialization
            .and_then(|specialization| specialization.get(self.db, paramspec))
        else {
            return false;
        };

        if callable.kind(self.db) != CallableTypeKind::ParamSpecValue {
            return false;
        }

        // TODO: Support overloads?
        let [signature] = callable.signatures(self.db).overloads.as_slice() else {
            return false;
        };

        let sub_arguments = if let Some(argument_index) = argument_index {
            self.arguments.start_from(argument_index)
        } else {
            CallArguments::none()
        };

        // TODO: What should be the `signature_type` here?
        let bindings = match Bindings::from(Binding::single(self.signature_type, signature.clone()))
            .match_parameters(self.db, &sub_arguments)
            .check_types(self.db, &sub_arguments, self.call_expression_tcx, &[])
        {
            Ok(bindings) => Box::new(bindings),
            Err(CallError(_, bindings)) => bindings,
        };

        // SAFETY: `bindings` was created from a single binding above.
        let [binding] = bindings.single_element().unwrap().overloads.as_slice() else {
            unreachable!("ParamSpec sub-call should only contain a single binding");
        };

        self.errors.extend(binding.errors.iter().cloned());

        true
    }

    fn check_variadic_argument_type(
        &mut self,
        argument_index: usize,
        adjusted_argument_index: Option<usize>,
        argument: Argument<'a>,
    ) {
        for (parameter_index, variadic_argument_type) in
            self.argument_matches[argument_index].iter()
        {
            self.check_argument_type(
                adjusted_argument_index,
                argument,
                variadic_argument_type.unwrap_or_else(Type::unknown),
                parameter_index,
            );
        }
    }

    fn check_keyword_variadic_argument_type(
        &mut self,
        argument_index: usize,
        adjusted_argument_index: Option<usize>,
        argument: Argument<'a>,
        argument_type: Type<'db>,
    ) {
        if let Type::TypedDict(typed_dict) = argument_type {
            for (argument_type, parameter_index) in typed_dict
                .items(self.db)
                .values()
                .map(|field| field.declared_ty)
                .zip(&self.argument_matches[argument_index].parameters)
            {
                self.check_argument_type(
                    adjusted_argument_index,
                    argument,
                    argument_type,
                    *parameter_index,
                );
            }
        } else {
            let mut value_type_fallback = |argument_type: Type<'db>| {
                // TODO: Instead of calling the `keys` and `__getitem__` methods, we should
                // instead get the constraints which satisfies the `SupportsKeysAndGetItem`
                // protocol i.e., the key and value type.
                let key_type = match argument_type
                    .member_lookup_with_policy(
                        self.db,
                        Name::new_static("keys"),
                        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                    )
                    .place
                {
                    Place::Defined(keys_method, _, Definedness::AlwaysDefined) => keys_method
                        .try_call(self.db, &CallArguments::none())
                        .ok()
                        .and_then(|bindings| {
                            Some(
                                bindings
                                    .return_type(self.db)
                                    .try_iterate(self.db)
                                    .ok()?
                                    .homogeneous_element_type(self.db),
                            )
                        }),
                    _ => None,
                };

                let Some(key_type) = key_type else {
                    self.errors.push(BindingError::KeywordsNotAMapping {
                        argument_index: adjusted_argument_index,
                        provided_ty: argument_type,
                    });
                    return None;
                };

                if !key_type
                    .when_assignable_to(
                        self.db,
                        KnownClass::Str.to_instance(self.db),
                        self.inferable_typevars,
                    )
                    .is_always_satisfied(self.db)
                {
                    self.errors.push(BindingError::InvalidKeyType {
                        argument_index: adjusted_argument_index,
                        provided_ty: key_type,
                    });
                }

                Some(
                    match argument_type
                        .member_lookup_with_policy(
                            self.db,
                            Name::new_static("__getitem__"),
                            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                        )
                        .place
                    {
                        Place::Defined(keys_method, _, Definedness::AlwaysDefined) => keys_method
                            .try_call(self.db, &CallArguments::positional([Type::unknown()]))
                            .ok()
                            .map_or_else(Type::unknown, |bindings| bindings.return_type(self.db)),
                        _ => Type::unknown(),
                    },
                )
            };

            let value_type = match argument_type {
                Type::Union(union) => {
                    // See the comment in `match_variadic` for why we special case this situation.
                    match union.elements(self.db) {
                        [paramspec @ Type::TypeVar(typevar), other]
                        | [other, paramspec @ Type::TypeVar(typevar)]
                            if typevar.is_paramspec(self.db) && other.is_unknown() =>
                        {
                            Some(*paramspec)
                        }
                        _ => value_type_fallback(argument_type),
                    }
                }
                Type::TypeVar(typevar) if typevar.is_paramspec(self.db) => Some(argument_type),
                _ => value_type_fallback(argument_type),
            };

            let Some(value_type) = value_type else {
                return;
            };

            for (argument_type, parameter_index) in
                std::iter::repeat(value_type).zip(&self.argument_matches[argument_index].parameters)
            {
                self.check_argument_type(
                    adjusted_argument_index,
                    Argument::Keywords,
                    argument_type,
                    *parameter_index,
                );
            }
        }
    }

    fn finish(
        self,
    ) -> (
        InferableTypeVars<'db, 'db>,
        Option<Specialization<'db>>,
        Type<'db>,
    ) {
        (self.inferable_typevars, self.specialization, self.return_ty)
    }
}

/// Information about which parameter(s) an argument was matched against. This is tracked
/// separately for each overload.
#[derive(Clone, Debug, Default)]
pub struct MatchedArgument<'db> {
    /// The index of the parameter(s) that an argument was matched against. A splatted argument
    /// might be matched against multiple parameters.
    pub parameters: SmallVec<[usize; 1]>,

    /// Whether there were errors matching this argument. For a splatted argument, _all_ splatted
    /// elements must have been successfully matched. (That means that this can be `false` while
    /// the `parameters` field is non-empty.)
    pub matched: bool,

    /// The types of a variadic argument when it's unpacked.
    ///
    /// The length of this vector is always the same as the `parameters` vector i.e., these are the
    /// types assigned to each matched parameter. This isn't necessarily the same as the number of
    /// types in the argument type which might not be a fixed-length iterable.
    ///
    /// Another thing to note is that the way this is populated means that for any other argument
    /// kind (synthetic, positional, keyword, keyword-variadic), this will be a single-element
    /// vector containing `None`, since we don't know the type of the argument when this is
    /// constructed. So, this field is populated only for variadic arguments.
    ///
    /// For example, given a `*args` whose type is `tuple[A, B, C]` and the following parameters:
    /// - `(x, *args)`: the `types` field will only have two elements (`B`, `C`) since `A` has been
    ///   matched with `x`.
    /// - `(*args)`: the `types` field will have all the three elements (`A`, `B`, `C`)
    types: SmallVec<[Option<Type<'db>>; 1]>,
}

impl<'db> MatchedArgument<'db> {
    /// Returns an iterator over the parameter indices and the corresponding argument type.
    pub fn iter(&self) -> impl Iterator<Item = (usize, Option<Type<'db>>)> + '_ {
        self.parameters
            .iter()
            .copied()
            .zip(self.types.iter().copied())
    }
}

/// Indicates that a parameter of the given name was not found.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UnknownParameterNameError;

/// Binding information for one of the overloads of a callable.
#[derive(Debug, Clone)]
pub(crate) struct Binding<'db> {
    pub(crate) signature: Signature<'db>,

    /// The type that is (hopefully) callable.
    pub(crate) callable_type: Type<'db>,

    /// The type we'll use for error messages referring to details of the called signature. For
    /// calls to functions this will be the same as `callable_type`; for other callable instances
    /// it may be a `__call__` method.
    pub(crate) signature_type: Type<'db>,

    /// The type of the instance being constructed, if this signature is for a constructor.
    pub(crate) constructor_instance_type: Option<Type<'db>>,

    /// Return type of the call.
    return_ty: Type<'db>,

    /// The inferable typevars in this signature.
    inferable_typevars: InferableTypeVars<'db, 'db>,

    /// The specialization that was inferred from the argument types, if the callable is generic.
    specialization: Option<Specialization<'db>>,

    /// Information about which parameter(s) each argument was matched with, in argument source
    /// order.
    argument_matches: Box<[MatchedArgument<'db>]>,

    /// Whether an argument that supplies an indeterminate number of positional or keyword
    /// arguments is mapped to a variadic parameter (`*args` or `**kwargs`).
    variadic_argument_matched_to_variadic_parameter: bool,

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
            constructor_instance_type: None,
            return_ty: Type::unknown(),
            inferable_typevars: InferableTypeVars::None,
            specialization: None,
            argument_matches: Box::from([]),
            variadic_argument_matched_to_variadic_parameter: false,
            parameter_tys: Box::from([]),
            errors: vec![],
        }
    }

    fn replace_callable_type(&mut self, before: Type<'db>, after: Type<'db>) {
        if self.callable_type == before {
            self.callable_type = after;
        }
    }

    fn match_parameters(
        &mut self,
        db: &'db dyn Db,
        arguments: &CallArguments<'_, 'db>,
        argument_forms: &mut ArgumentForms,
    ) {
        let parameters = self.signature.parameters();
        let mut matcher =
            ArgumentMatcher::new(arguments, parameters, argument_forms, &mut self.errors);
        let mut keywords_arguments = vec![];
        for (argument_index, (argument, argument_type)) in arguments.iter().enumerate() {
            match argument {
                Argument::Positional | Argument::Synthetic => {
                    let _ = matcher.match_positional(argument_index, argument, None, false);
                }
                Argument::Keyword(name) => {
                    let _ = matcher.match_keyword(argument_index, argument, None, name);
                }
                Argument::Variadic => {
                    let _ = matcher.match_variadic(db, argument_index, argument, argument_type);
                }
                Argument::Keywords => {
                    keywords_arguments.push((argument_index, argument_type));
                }
            }
        }
        for (keywords_index, keywords_type) in keywords_arguments {
            matcher.match_keyword_variadic(db, keywords_index, keywords_type);
        }
        self.return_ty = self.signature.return_ty.unwrap_or(Type::unknown());
        self.parameter_tys = vec![None; parameters.len()].into_boxed_slice();
        self.variadic_argument_matched_to_variadic_parameter =
            matcher.variadic_argument_matched_to_variadic_parameter;
        self.argument_matches = matcher.finish();
    }

    fn check_types(
        &mut self,
        db: &'db dyn Db,
        arguments: &CallArguments<'_, 'db>,
        call_expression_tcx: TypeContext<'db>,
    ) {
        let mut checker = ArgumentTypeChecker::new(
            db,
            self.signature_type,
            &self.signature,
            arguments,
            &self.argument_matches,
            &mut self.parameter_tys,
            self.constructor_instance_type,
            call_expression_tcx,
            self.return_ty,
            &mut self.errors,
        );

        // If this overload is generic, first see if we can infer a specialization of the function
        // from the arguments that were passed in.
        checker.infer_specialization();
        checker.check_argument_types();

        (self.inferable_typevars, self.specialization, self.return_ty) = checker.finish();
    }

    pub(crate) fn set_return_type(&mut self, return_ty: Type<'db>) {
        self.return_ty = return_ty;
    }

    pub(crate) fn return_type(&self) -> Type<'db> {
        self.return_ty
    }

    pub(crate) fn specialization(&self) -> Option<Specialization<'db>> {
        self.specialization
    }

    /// Returns the bound types for each parameter, in parameter source order, or `None` if no
    /// argument was matched to that parameter.
    pub(crate) fn parameter_types(&self) -> &[Option<Type<'db>>] {
        &self.parameter_tys
    }

    /// Returns the bound type for the specified parameter, or `None` if no argument was matched to
    /// that parameter.
    ///
    /// Returns an error if the parameter name is not found.
    pub(crate) fn parameter_type_by_name(
        &self,
        parameter_name: &str,
        fallback_to_default: bool,
    ) -> Result<Option<Type<'db>>, UnknownParameterNameError> {
        let parameters = self.signature.parameters();

        let index = parameters
            .keyword_by_name(parameter_name)
            .map(|(i, _)| i)
            .ok_or(UnknownParameterNameError)?;

        let parameter_ty = self.parameter_tys[index];

        if parameter_ty.is_some() {
            Ok(parameter_ty)
        } else if fallback_to_default {
            Ok(parameters[index].default_type())
        } else {
            Ok(None)
        }
    }

    pub(crate) fn arguments_for_parameter<'a>(
        &'a self,
        argument_types: &'a CallArguments<'a, 'db>,
        parameter_index: usize,
    ) -> impl Iterator<Item = (Argument<'a>, Type<'db>)> + 'a {
        argument_types
            .iter()
            .zip(&self.argument_matches)
            .filter(move |(_, argument_matches)| {
                argument_matches.parameters.contains(&parameter_index)
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
            inferable_typevars: self.inferable_typevars,
            specialization: self.specialization,
            argument_matches: self.argument_matches.clone(),
            parameter_tys: self.parameter_tys.clone(),
            errors: self.errors.clone(),
        }
    }

    fn restore(&mut self, snapshot: BindingSnapshot<'db>) {
        let BindingSnapshot {
            return_ty,
            inferable_typevars,
            specialization,
            argument_matches,
            parameter_tys,
            errors,
        } = snapshot;

        self.return_ty = return_ty;
        self.inferable_typevars = inferable_typevars;
        self.specialization = specialization;
        self.argument_matches = argument_matches;
        self.parameter_tys = parameter_tys;
        self.errors = errors;
    }

    /// Returns a vector where each index corresponds to an argument position,
    /// and the value is the parameter index that argument maps to (if any).
    pub(crate) fn argument_matches(&self) -> &[MatchedArgument<'db>] {
        &self.argument_matches
    }

    pub(crate) fn errors(&self) -> &[BindingError<'db>] {
        &self.errors
    }

    /// Resets the state of this binding to its initial state.
    fn reset(&mut self) {
        self.return_ty = Type::unknown();
        self.inferable_typevars = InferableTypeVars::None;
        self.specialization = None;
        self.argument_matches = Box::from([]);
        self.parameter_tys = Box::from([]);
        self.errors.clear();
    }
}

#[derive(Clone, Debug)]
struct BindingSnapshot<'db> {
    return_ty: Type<'db>,
    inferable_typevars: InferableTypeVars<'db, 'db>,
    specialization: Option<Specialization<'db>>,
    argument_matches: Box<[MatchedArgument<'db>]>,
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
                snapshot.inferable_typevars = binding.inferable_typevars;
                snapshot.specialization = binding.specialization;
                snapshot
                    .argument_matches
                    .clone_from(&binding.argument_matches);
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
    pub(crate) name: &'a str,
    pub(crate) kind: &'a str,
}

impl<'db> CallableDescription<'db> {
    pub(crate) fn new(
        db: &'db dyn Db,
        callable_type: Type<'db>,
    ) -> Option<CallableDescription<'db>> {
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
            Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(function)) => {
                Some(CallableDescription {
                    kind: "method wrapper `__get__` of function",
                    name: function.name(db),
                })
            }
            Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderGet(_)) => {
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
    /// The type of the keyword-variadic argument's key is not `str`.
    InvalidKeyType {
        argument_index: Option<usize>,
        provided_ty: Type<'db>,
    },
    KeywordsNotAMapping {
        argument_index: Option<usize>,
        provided_ty: Type<'db>,
    },
    /// One or more required parameters (that is, with no default) is not supplied by any argument.
    MissingArguments {
        parameters: ParameterContexts,
    },
    /// A call argument can't be matched to any parameter.
    UnknownArgument {
        argument_name: ast::name::Name,
        argument_index: Option<usize>,
    },
    /// A positional-only parameter is passed as keyword argument.
    PositionalOnlyParameterAsKwarg {
        argument_index: Option<usize>,
        parameter: ParameterContext,
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
    PropertyHasNoSetter(PropertyInstanceType<'db>),
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
        let callable_kind = match callable_ty {
            Type::FunctionLiteral(_) => "Function",
            Type::BoundMethod(_) => "Method",
            _ => "Callable",
        };

        match self {
            Self::InvalidArgumentType {
                parameter,
                argument_index,
                expected_ty,
                provided_ty,
            } => {
                // Certain special forms in the typing module are aliases for classes
                // elsewhere in the standard library. These special forms are not instances of `type`,
                // and you cannot use them in place of their aliased classes in *all* situations:
                // for example, `dict()` succeeds at runtime, but `typing.Dict()` fails. However,
                // they *can* all be used as the second argument to `isinstance` and `issubclass`.
                // We model that specific aspect of their behaviour here.
                //
                // This is implemented as a special case in call-binding machinery because overriding
                // typeshed's signatures for `isinstance()` and `issubclass()` would be complex and
                // error-prone, due to the fact that they are annotated with recursive type aliases.
                if parameter.index == 1
                    && *argument_index == Some(1)
                    && matches!(
                        callable_ty
                            .as_function_literal()
                            .and_then(|function| function.known(context.db())),
                        Some(KnownFunction::IsInstance | KnownFunction::IsSubclass)
                    )
                    && provided_ty
                        .as_special_form()
                        .is_some_and(SpecialFormType::is_valid_isinstance_target)
                {
                    return;
                }

                // TODO: Ideally we would not emit diagnostics for `TypedDict` literal arguments
                // here (see `diagnostic::is_invalid_typed_dict_literal`). However, we may have
                // silenced diagnostics during overload evaluation, and rely on the assignability
                // diagnostic being emitted here.

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

                if let Type::Union(union) = provided_ty {
                    let union_elements = union.elements(context.db());
                    let invalid_elements: Vec<Type<'db>> = union
                        .elements(context.db())
                        .iter()
                        .filter(|element| !element.is_assignable_to(context.db(), *expected_ty))
                        .copied()
                        .collect();
                    let first_invalid_element = invalid_elements[0].display(context.db());
                    if invalid_elements.len() < union_elements.len() {
                        match &invalid_elements[1..] {
                            [] => diag.info(format_args!(
                                "Element `{first_invalid_element}` of this union \
                                is not assignable to `{expected_ty_display}`",
                            )),
                            [single] => diag.info(format_args!(
                                "Union elements `{first_invalid_element}` and `{}` \
                                are not assignable to `{expected_ty_display}`",
                                single.display(context.db()),
                            )),
                            rest => diag.info(format_args!(
                                "Union element `{first_invalid_element}`, \
                                and {} more union elements, \
                                are not assignable to `{expected_ty_display}`",
                                rest.len(),
                            )),
                        }
                    }
                }

                if let Some(matching_overload) = matching_overload {
                    if let Some((name_span, parameter_span)) =
                        matching_overload.get(context.db()).and_then(|overload| {
                            overload.parameter_span(context.db(), Some(parameter.index))
                        })
                    {
                        let mut sub = SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            "Matching overload defined here",
                        );
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
                                overload.signature(context.db()).display(context.db())
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
                    let mut sub = SubDiagnostic::new(
                        SubDiagnosticSeverity::Info,
                        format_args!("{callable_kind} defined here"),
                    );
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

            Self::InvalidKeyType {
                argument_index,
                provided_ty,
            } => {
                let range = Self::get_node(node, *argument_index);
                let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, range) else {
                    return;
                };

                let provided_ty_display = provided_ty.display(context.db());
                let mut diag = builder.into_diagnostic(
                    "Argument expression after ** must be a mapping with `str` key type",
                );
                diag.set_primary_message(format_args!("Found `{provided_ty_display}`"));

                if let Some(union_diag) = union_diag {
                    union_diag.add_union_context(context.db(), &mut diag);
                }
            }

            Self::KeywordsNotAMapping {
                argument_index,
                provided_ty,
            } => {
                let range = Self::get_node(node, *argument_index);
                let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, range) else {
                    return;
                };

                let provided_ty_display = provided_ty.display(context.db());
                let mut diag =
                    builder.into_diagnostic("Argument expression after ** must be a mapping type");
                diag.set_primary_message(format_args!("Found `{provided_ty_display}`"));

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
                    } else if let Some(spans) = callable_ty.function_spans(context.db()) {
                        let mut sub = SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            format_args!("{callable_kind} signature here"),
                        );
                        sub.annotate(Annotation::primary(spans.signature));
                        diag.sub(sub);
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
                    } else {
                        let span = callable_ty.parameter_span(
                            context.db(),
                            (parameters.0.len() == 1).then(|| parameters.0[0].index),
                        );
                        if let Some((_, parameter_span)) = span {
                            let mut sub = SubDiagnostic::new(
                                SubDiagnosticSeverity::Info,
                                format_args!("Parameter{s} declared here"),
                            );
                            sub.annotate(Annotation::primary(parameter_span));
                            diag.sub(sub);
                        }
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
                    } else if let Some(spans) = callable_ty.function_spans(context.db()) {
                        let mut sub = SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            format_args!("{callable_kind} signature here"),
                        );
                        sub.annotate(Annotation::primary(spans.signature));
                        diag.sub(sub);
                    }
                }
            }

            Self::PositionalOnlyParameterAsKwarg {
                argument_index,
                parameter,
            } => {
                let node = Self::get_node(node, *argument_index);
                if let Some(builder) =
                    context.report_lint(&POSITIONAL_ONLY_PARAMETER_AS_KWARG, node)
                {
                    let mut diag = builder.into_diagnostic(format_args!(
                        "Positional-only parameter {parameter} passed as keyword argument{}",
                        if let Some(CallableDescription { kind, name }) = callable_description {
                            format!(" of {kind} `{name}`")
                        } else {
                            String::new()
                        }
                    ));
                    if let Some(union_diag) = union_diag {
                        union_diag.add_union_context(context.db(), &mut diag);
                    } else if let Some(spans) = callable_ty.function_spans(context.db()) {
                        let mut sub = SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            format_args!("{callable_kind} signature here"),
                        );
                        sub.annotate(Annotation::primary(spans.signature));
                        diag.sub(sub);
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

                let typevar = error.bound_typevar().typevar(context.db());
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

                let typevar_name = typevar.name(context.db());
                match error {
                    SpecializationError::MismatchedBound { .. } => {
                        diag.set_primary_message(format_args!("Argument type `{argument_ty_display}` does not satisfy upper bound `{}` of type variable `{typevar_name}`",
                        typevar.upper_bound(context.db()).expect("type variable should have an upper bound if this error occurs").display(context.db())
                    ));
                    }
                    SpecializationError::MismatchedConstraint { .. } => {
                        diag.set_primary_message(format_args!("Argument type `{argument_ty_display}` does not satisfy constraints ({}) of type variable `{typevar_name}`",
                        typevar.constraints(context.db()).expect("type variable should have constraints if this error occurs").iter().map(|ty| format!("`{}`", ty.display(context.db()))).join(", ")
                    ));
                    }
                }

                if let Some(typevar_definition) = typevar.definition(context.db()) {
                    let module = parsed_module(context.db(), typevar_definition.file(context.db()))
                        .load(context.db());
                    let typevar_range = typevar_definition.full_range(context.db(), &module);
                    let mut sub = SubDiagnostic::new(
                        SubDiagnosticSeverity::Info,
                        "Type variable defined here",
                    );
                    sub.annotate(Annotation::primary(typevar_range.into()));
                    diag.sub(sub);
                }

                if let Some(union_diag) = union_diag {
                    union_diag.add_union_context(context.db(), &mut diag);
                }
            }

            Self::PropertyHasNoSetter(_) => {
                BindingError::InternalCallError("property has no setter").report_diagnostic(
                    context,
                    node,
                    callable_ty,
                    callable_description,
                    union_diag,
                    matching_overload,
                );
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

    fn get_node(node: ast::AnyNodeRef<'_>, argument_index: Option<usize>) -> ast::AnyNodeRef<'_> {
        // If we have a Call node and an argument index, report the diagnostic on the correct
        // argument node; otherwise, report it on the entire provided node.
        match Self::get_argument_node(node, argument_index) {
            Some(ast::ArgOrKeyword::Arg(expr)) => expr.into(),
            Some(ast::ArgOrKeyword::Keyword(expr)) => expr.into(),
            None => node,
        }
    }

    fn get_argument_node(
        node: ast::AnyNodeRef<'_>,
        argument_index: Option<usize>,
    ) -> Option<ArgOrKeyword<'_>> {
        match (node, argument_index) {
            (ast::AnyNodeRef::ExprCall(call_node), Some(argument_index)) => Some(
                call_node
                    .arguments
                    .arguments_source_order()
                    .nth(argument_index)
                    .expect("argument index should not be out of range"),
            ),
            _ => None,
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
            SubDiagnosticSeverity::Info,
            format_args!(
                "Union variant `{callable_ty}` is incompatible with this call site",
                callable_ty = self.binding.callable_type.display(db),
            ),
        );
        diag.sub(sub);

        let sub = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
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
