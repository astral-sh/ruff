//! When analyzing a call site, we create _bindings_, which match and type-check the actual
//! arguments against the parameters of the callable. Like with
//! [signatures][crate::types::signatures], we have to handle the fact that the callable might be a
//! union of types, each of which might contain multiple overloads.

use std::borrow::Cow;

use smallvec::SmallVec;

use super::{
    Argument, CallArguments, CallError, CallErrorKind, CallableSignature, InferContext, Signature,
    Signatures, Type,
};
use crate::db::Db;
use crate::types::diagnostic::{
    CALL_NON_CALLABLE, INVALID_ARGUMENT_TYPE, MISSING_ARGUMENT, NO_MATCHING_OVERLOAD,
    PARAMETER_ALREADY_ASSIGNED, TOO_MANY_POSITIONAL_ARGUMENTS, UNKNOWN_ARGUMENT,
};
use crate::types::signatures::Parameter;
use crate::types::{CallableType, UnionType};
use ruff_db::diagnostic::{OldSecondaryDiagnosticMessage, Span};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

/// Binding information for a possible union of callables. At a call site, the arguments must be
/// compatible with _all_ of the types in the union for the call to be valid.
///
/// It's guaranteed that the wrapped bindings have no errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Bindings<'db> {
    pub(crate) callable_type: Type<'db>,
    /// By using `SmallVec`, we avoid an extra heap allocation for the common case of a non-union
    /// type.
    elements: SmallVec<[CallableBinding<'db>; 1]>,
}

impl<'db> Bindings<'db> {
    /// Binds the arguments of a call site against a signature.
    ///
    /// The returned bindings provide the return type of the call, the bound types for all
    /// parameters, and any errors resulting from binding the call, all for each union element and
    /// overload (if any).
    pub(crate) fn bind(
        db: &'db dyn Db,
        signatures: &Signatures<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> Result<Self, CallError<'db>> {
        let elements: SmallVec<[CallableBinding<'db>; 1]> = signatures
            .into_iter()
            .map(|signature| CallableBinding::bind(db, signature, arguments))
            .collect();

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
        for binding in &elements {
            let result = binding.as_result();
            all_ok &= result.is_ok();
            any_binding_error |= matches!(result, Err(CallErrorKind::BindingError));
            all_not_callable &= matches!(result, Err(CallErrorKind::NotCallable));
        }

        let bindings = Bindings {
            callable_type: signatures.callable_type,
            elements,
        };

        if all_ok {
            Ok(bindings)
        } else if any_binding_error {
            Err(CallError(CallErrorKind::BindingError, Box::new(bindings)))
        } else if all_not_callable {
            Err(CallError(CallErrorKind::NotCallable, Box::new(bindings)))
        } else {
            Err(CallError(
                CallErrorKind::PossiblyNotCallable,
                Box::new(bindings),
            ))
        }
    }

    pub(crate) fn is_single(&self) -> bool {
        self.elements.len() == 1
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
            context.report_lint(
                &CALL_NON_CALLABLE,
                node,
                format_args!(
                    "Object of type `{}` is not callable",
                    self.callable_type.display(context.db())
                ),
            );
            return;
        }

        // TODO: We currently only report errors for the first union element. Ideally, we'd report
        // an error saying that the union type can't be called, followed by subdiagnostics
        // explaining why.
        if let Some(first) = self.into_iter().find(|b| b.as_result().is_err()) {
            first.report_diagnostics(context, node);
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Bind a [`CallArguments`] against a [`CallableSignature`].
    ///
    /// The returned [`CallableBinding`] provides the return type of the call, the bound types for
    /// all parameters, and any errors resulting from binding the call.
    fn bind(
        db: &'db dyn Db,
        signature: &CallableSignature<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> Self {
        // If this callable is a bound method, prepend the self instance onto the arguments list
        // before checking.
        let arguments = if let Some(bound_type) = signature.bound_type {
            Cow::Owned(arguments.with_self(bound_type))
        } else {
            Cow::Borrowed(arguments)
        };

        // TODO: This checks every overload. In the proposed more detailed call checking spec [1],
        // arguments are checked for arity first, and are only checked for type assignability against
        // the matching overloads. Make sure to implement that as part of separating call binding into
        // two phases.
        //
        // [1] https://github.com/python/typing/pull/1839
        let overloads = signature
            .into_iter()
            .map(|signature| Binding::bind(db, signature, arguments.as_ref()))
            .collect();
        CallableBinding {
            callable_type: signature.callable_type,
            signature_type: signature.signature_type,
            dunder_call_is_possibly_unbound: signature.dunder_call_is_possibly_unbound,
            overloads,
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
        self.matching_overload().is_none()
    }

    /// Returns the overload that matched for this call binding. Returns `None` if none of the
    /// overloads matched.
    pub(crate) fn matching_overload(&self) -> Option<(usize, &Binding<'db>)> {
        self.overloads
            .iter()
            .enumerate()
            .find(|(_, overload)| overload.as_result().is_ok())
    }

    /// Returns the overload that matched for this call binding. Returns `None` if none of the
    /// overloads matched.
    pub(crate) fn matching_overload_mut(&mut self) -> Option<(usize, &mut Binding<'db>)> {
        self.overloads
            .iter_mut()
            .enumerate()
            .find(|(_, overload)| overload.as_result().is_ok())
    }

    /// Returns the return type of this call. For a valid call, this is the return type of the
    /// overload that the arguments matched against. For an invalid call to a non-overloaded
    /// function, this is the return type of the function. For an invalid call to an overloaded
    /// function, we return `Type::unknown`, since we cannot make any useful conclusions about
    /// which overload was intended to be called.
    pub(crate) fn return_type(&self) -> Type<'db> {
        if let Some((_, overload)) = self.matching_overload() {
            return overload.return_type();
        }
        if let [overload] = self.overloads.as_slice() {
            return overload.return_type();
        }
        Type::unknown()
    }

    fn report_diagnostics(&self, context: &InferContext<'db>, node: ast::AnyNodeRef) {
        if !self.is_callable() {
            context.report_lint(
                &CALL_NON_CALLABLE,
                node,
                format_args!(
                    "Object of type `{}` is not callable",
                    self.callable_type.display(context.db()),
                ),
            );
            return;
        }

        if self.dunder_call_is_possibly_unbound {
            context.report_lint(
                &CALL_NON_CALLABLE,
                node,
                format_args!(
                    "Object of type `{}` is not callable (possibly unbound `__call__` method)",
                    self.callable_type.display(context.db()),
                ),
            );
            return;
        }

        let callable_description = CallableDescription::new(context.db(), self.callable_type);
        if self.overloads.len() > 1 {
            context.report_lint(
                &NO_MATCHING_OVERLOAD,
                node,
                format_args!(
                    "No overload{} matches arguments",
                    if let Some(CallableDescription { kind, name }) = callable_description {
                        format!(" of {kind} `{name}`")
                    } else {
                        String::new()
                    }
                ),
            );
            return;
        }

        let callable_description = CallableDescription::new(context.db(), self.signature_type);
        for overload in &self.overloads {
            overload.report_diagnostics(
                context,
                node,
                self.signature_type,
                callable_description.as_ref(),
            );
        }
    }
}

/// Binding information for one of the overloads of a callable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Binding<'db> {
    /// Return type of the call.
    return_ty: Type<'db>,

    /// Bound types for parameters, in parameter source order.
    parameter_tys: Box<[Type<'db>]>,

    /// Call binding errors, if any.
    errors: Vec<BindingError<'db>>,
}

impl<'db> Binding<'db> {
    fn bind(
        db: &'db dyn Db,
        signature: &Signature<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> Self {
        let parameters = signature.parameters();
        // The type assigned to each parameter at this call site.
        let mut parameter_tys = vec![None; parameters.len()];
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
            let (index, parameter, argument_ty, positional) = match argument {
                Argument::Positional(ty) | Argument::Synthetic(ty) => {
                    if matches!(argument, Argument::Synthetic(_)) {
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
                    (index, parameter, ty, !parameter.is_variadic())
                }
                Argument::Keyword { name, ty } => {
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
                    (index, parameter, ty, false)
                }

                Argument::Variadic(_) | Argument::Keywords(_) => {
                    // TODO
                    continue;
                }
            };
            if let Some(expected_ty) = parameter.annotated_type() {
                if !argument_ty.is_assignable_to(db, expected_ty) {
                    errors.push(BindingError::InvalidArgumentType {
                        parameter: ParameterContext::new(parameter, index, positional),
                        argument_index: get_argument_index(argument_index, num_synthetic_args),
                        expected_ty,
                        provided_ty: *argument_ty,
                    });
                }
            }
            if let Some(existing) = parameter_tys[index].replace(*argument_ty) {
                if parameter.is_variadic() || parameter.is_keyword_variadic() {
                    let union = UnionType::from_elements(db, [existing, *argument_ty]);
                    parameter_tys[index].replace(union);
                } else {
                    errors.push(BindingError::ParameterAlreadyAssigned {
                        argument_index: get_argument_index(argument_index, num_synthetic_args),
                        parameter: ParameterContext::new(parameter, index, positional),
                    });
                }
            }
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
        for (index, bound_ty) in parameter_tys.iter().enumerate() {
            if bound_ty.is_none() {
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
            parameter_tys: parameter_tys
                .into_iter()
                .map(|opt_ty| opt_ty.unwrap_or(Type::unknown()))
                .collect(),
            errors,
        }
    }

    pub(crate) fn set_return_type(&mut self, return_ty: Type<'db>) {
        self.return_ty = return_ty;
    }

    pub(crate) fn return_type(&self) -> Type<'db> {
        self.return_ty
    }

    pub(crate) fn parameter_types(&self) -> &[Type<'db>] {
        &self.parameter_tys
    }

    fn report_diagnostics(
        &self,
        context: &InferContext<'db>,
        node: ast::AnyNodeRef,
        callable_ty: Type<'db>,
        callable_description: Option<&CallableDescription>,
    ) {
        for error in &self.errors {
            error.report_diagnostic(context, node, callable_ty, callable_description);
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
                name: class_type.class().name(db),
            }),
            Type::Callable(CallableType::BoundMethod(bound_method)) => Some(CallableDescription {
                kind: "bound method",
                name: bound_method.function(db).name(db),
            }),
            Type::Callable(CallableType::MethodWrapperDunderGet(function)) => {
                Some(CallableDescription {
                    kind: "method wrapper `__get__` of function",
                    name: function.name(db),
                })
            }
            Type::Callable(CallableType::WrapperDescriptorDunderGet) => Some(CallableDescription {
                kind: "wrapper descriptor",
                name: "FunctionType.__get__",
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
}

impl<'db> BindingError<'db> {
    fn parameter_span_from_index(
        db: &'db dyn Db,
        callable_ty: Type<'db>,
        parameter_index: usize,
    ) -> Option<Span> {
        match callable_ty {
            Type::FunctionLiteral(function) => {
                let function_scope = function.body_scope(db);
                let mut span = Span::from(function_scope.file(db));
                let node = function_scope.node(db);
                if let Some(func_def) = node.as_function() {
                    let range = func_def
                        .parameters
                        .iter()
                        .nth(parameter_index)
                        .map(|param| param.range())
                        .unwrap_or(func_def.parameters.range);
                    span = span.with_range(range);
                    Some(span)
                } else {
                    None
                }
            }
            Type::Callable(CallableType::BoundMethod(bound_method)) => {
                Self::parameter_span_from_index(
                    db,
                    Type::FunctionLiteral(bound_method.function(db)),
                    parameter_index,
                )
            }
            _ => None,
        }
    }

    pub(super) fn report_diagnostic(
        &self,
        context: &InferContext<'db>,
        node: ast::AnyNodeRef,
        callable_ty: Type<'db>,
        callable_description: Option<&CallableDescription>,
    ) {
        match self {
            Self::InvalidArgumentType {
                parameter,
                argument_index,
                expected_ty,
                provided_ty,
            } => {
                let mut messages = vec![];
                if let Some(span) =
                    Self::parameter_span_from_index(context.db(), callable_ty, parameter.index)
                {
                    messages.push(OldSecondaryDiagnosticMessage::new(
                        span,
                        "parameter declared in function definition here",
                    ));
                }

                let provided_ty_display = provided_ty.display(context.db());
                let expected_ty_display = expected_ty.display(context.db());
                context.report_lint_with_secondary_messages(
                    &INVALID_ARGUMENT_TYPE,
                    Self::get_node(node, *argument_index),
                    format_args!(
                        "Object of type `{provided_ty_display}` cannot be assigned to \
                        parameter {parameter}{}; expected type `{expected_ty_display}`",
                        if let Some(CallableDescription { kind, name }) = callable_description {
                            format!(" of {kind} `{name}`")
                        } else {
                            String::new()
                        }
                    ),
                    messages,
                );
            }

            Self::TooManyPositionalArguments {
                first_excess_argument_index,
                expected_positional_count,
                provided_positional_count,
            } => {
                context.report_lint(
                    &TOO_MANY_POSITIONAL_ARGUMENTS,
                    Self::get_node(node, *first_excess_argument_index),
                    format_args!(
                        "Too many positional arguments{}: expected \
                        {expected_positional_count}, got {provided_positional_count}",
                        if let Some(CallableDescription { kind, name }) = callable_description {
                            format!(" to {kind} `{name}`")
                        } else {
                            String::new()
                        }
                    ),
                );
            }

            Self::MissingArguments { parameters } => {
                let s = if parameters.0.len() == 1 { "" } else { "s" };
                context.report_lint(
                    &MISSING_ARGUMENT,
                    node,
                    format_args!(
                        "No argument{s} provided for required parameter{s} {parameters}{}",
                        if let Some(CallableDescription { kind, name }) = callable_description {
                            format!(" of {kind} `{name}`")
                        } else {
                            String::new()
                        }
                    ),
                );
            }

            Self::UnknownArgument {
                argument_name,
                argument_index,
            } => {
                context.report_lint(
                    &UNKNOWN_ARGUMENT,
                    Self::get_node(node, *argument_index),
                    format_args!(
                        "Argument `{argument_name}` does not match any known parameter{}",
                        if let Some(CallableDescription { kind, name }) = callable_description {
                            format!(" of {kind} `{name}`")
                        } else {
                            String::new()
                        }
                    ),
                );
            }

            Self::ParameterAlreadyAssigned {
                argument_index,
                parameter,
            } => {
                context.report_lint(
                    &PARAMETER_ALREADY_ASSIGNED,
                    Self::get_node(node, *argument_index),
                    format_args!(
                        "Multiple values provided for parameter {parameter}{}",
                        if let Some(CallableDescription { kind, name }) = callable_description {
                            format!(" of {kind} `{name}`")
                        } else {
                            String::new()
                        }
                    ),
                );
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
