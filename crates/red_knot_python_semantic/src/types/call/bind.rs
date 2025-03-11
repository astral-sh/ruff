use super::{
    Argument, CallArguments, CallError, CallOutcome, InferContext, Overloads, Signature, Type,
};
use crate::db::Db;
use crate::types::diagnostic::{
    INVALID_ARGUMENT_TYPE, MISSING_ARGUMENT, NO_MATCHING_OVERLOAD, PARAMETER_ALREADY_ASSIGNED,
    TOO_MANY_POSITIONAL_ARGUMENTS, UNKNOWN_ARGUMENT,
};
use crate::types::signatures::Parameter;
use crate::types::{CallableType, UnionType};
use ruff_db::diagnostic::{OldSecondaryDiagnosticMessage, Span};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

/// Bind a [`CallArguments`] against a callable [`Signature`].
///
/// The returned [`CallBinding`] provides the return type of the call, the bound types for all
/// parameters, and any errors resulting from binding the call.
pub(crate) fn bind_call<'db>(
    db: &'db dyn Db,
    arguments: &CallArguments<'_, 'db>,
    overloads: &Overloads<'db>,
    callable_ty: Type<'db>,
) -> CallBinding<'db> {
    // TODO: This checks every overload. In the proposed more detailed call checking spec [1],
    // arguments are checked for arity first, and are only checked for type assignability against
    // the matching overloads. Make sure to implement that as part of separating call binding into
    // two phases.
    //
    // [1] https://github.com/python/typing/pull/1839
    let overloads = overloads
        .iter()
        .map(|signature| bind_overload(db, arguments, signature))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    CallBinding {
        callable_ty,
        overloads,
    }
}

fn bind_overload<'db>(
    db: &'db dyn Db,
    arguments: &CallArguments<'_, 'db>,
    signature: &Signature<'db>,
) -> OverloadBinding<'db> {
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
                    errors.push(CallBindingError::UnknownArgument {
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
                errors.push(CallBindingError::InvalidArgumentType {
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
                errors.push(CallBindingError::ParameterAlreadyAssigned {
                    argument_index: get_argument_index(argument_index, num_synthetic_args),
                    parameter: ParameterContext::new(parameter, index, positional),
                });
            }
        }
    }
    if let Some(first_excess_argument_index) = first_excess_positional {
        errors.push(CallBindingError::TooManyPositionalArguments {
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
            if param.is_variadic() || param.is_keyword_variadic() || param.default_type().is_some()
            {
                // variadic/keywords and defaulted arguments are not required
                continue;
            }
            missing.push(ParameterContext::new(param, index, false));
        }
    }

    if !missing.is_empty() {
        errors.push(CallBindingError::MissingArguments {
            parameters: ParameterContexts(missing),
        });
    }

    OverloadBinding {
        return_ty: signature.return_ty.unwrap_or(Type::unknown()),
        parameter_tys: parameter_tys
            .into_iter()
            .map(|opt_ty| opt_ty.unwrap_or(Type::unknown()))
            .collect(),
        errors,
    }
}

/// Describes a callable for the purposes of diagnostics.
#[derive(Debug)]
pub(crate) struct CallableDescriptor<'a> {
    name: &'a str,
    kind: &'a str,
}

/// Binding information for a call site.
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
pub(crate) struct CallBinding<'db> {
    /// Type of the callable object (function, class...)
    callable_ty: Type<'db>,

    overloads: Box<[OverloadBinding<'db>]>,
}

impl<'db> CallBinding<'db> {
    pub(crate) fn into_outcome(self) -> Result<CallOutcome<'db>, CallError<'db>> {
        if self.has_binding_errors() {
            return Err(CallError::BindingError { binding: self });
        }
        Ok(CallOutcome::Single(self))
    }

    pub(crate) fn callable_type(&self) -> Type<'db> {
        self.callable_ty
    }

    /// Returns whether there were any errors binding this call site. If the callable has multiple
    /// overloads, they must _all_ have errors.
    pub(crate) fn has_binding_errors(&self) -> bool {
        self.matching_overload().is_none()
    }

    /// Returns the overload that matched for this call binding. Returns `None` if none of the
    /// overloads matched.
    pub(crate) fn matching_overload(&self) -> Option<(usize, &OverloadBinding<'db>)> {
        self.overloads
            .iter()
            .enumerate()
            .find(|(_, overload)| !overload.has_binding_errors())
    }

    /// Returns the overload that matched for this call binding. Returns `None` if none of the
    /// overloads matched.
    pub(crate) fn matching_overload_mut(&mut self) -> Option<(usize, &mut OverloadBinding<'db>)> {
        self.overloads
            .iter_mut()
            .enumerate()
            .find(|(_, overload)| !overload.has_binding_errors())
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
        if let [overload] = self.overloads.as_ref() {
            return overload.return_type();
        }
        Type::unknown()
    }

    fn callable_descriptor(&self, db: &'db dyn Db) -> Option<CallableDescriptor> {
        match self.callable_ty {
            Type::FunctionLiteral(function) => Some(CallableDescriptor {
                kind: "function",
                name: function.name(db),
            }),
            Type::ClassLiteral(class_type) => Some(CallableDescriptor {
                kind: "class",
                name: class_type.class().name(db),
            }),
            Type::Callable(CallableType::BoundMethod(bound_method)) => Some(CallableDescriptor {
                kind: "bound method",
                name: bound_method.function(db).name(db),
            }),
            Type::Callable(CallableType::MethodWrapperDunderGet(function)) => {
                Some(CallableDescriptor {
                    kind: "method wrapper `__get__` of function",
                    name: function.name(db),
                })
            }
            Type::Callable(CallableType::WrapperDescriptorDunderGet) => Some(CallableDescriptor {
                kind: "wrapper descriptor",
                name: "FunctionType.__get__",
            }),
            _ => None,
        }
    }

    /// Report diagnostics for all of the errors that occurred when trying to match actual
    /// arguments to formal parameters. If the callable has multiple overloads, we report a single
    /// diagnostic that we couldn't match any overload.
    /// TODO: Update this to add subdiagnostics about how we failed to match each overload.
    pub(crate) fn report_diagnostics(&self, context: &InferContext<'db>, node: ast::AnyNodeRef) {
        let callable_descriptor = self.callable_descriptor(context.db());
        if self.overloads.len() > 1 {
            context.report_lint(
                &NO_MATCHING_OVERLOAD,
                node,
                format_args!(
                    "No overload{} matches arguments",
                    if let Some(CallableDescriptor { kind, name }) = callable_descriptor {
                        format!(" of {kind} `{name}`")
                    } else {
                        String::new()
                    }
                ),
            );
            return;
        }

        for overload in &self.overloads {
            overload.report_diagnostics(
                context,
                node,
                self.callable_ty,
                callable_descriptor.as_ref(),
            );
        }
    }
}

/// Binding information for one of the overloads of a callable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OverloadBinding<'db> {
    /// Return type of the call.
    return_ty: Type<'db>,

    /// Bound types for parameters, in parameter source order.
    parameter_tys: Box<[Type<'db>]>,

    /// Call binding errors, if any.
    errors: Vec<CallBindingError<'db>>,
}

impl<'db> OverloadBinding<'db> {
    pub(crate) fn set_return_type(&mut self, return_ty: Type<'db>) {
        self.return_ty = return_ty;
    }

    pub(crate) fn return_type(&self) -> Type<'db> {
        self.return_ty
    }

    pub(crate) fn parameter_types(&self) -> &[Type<'db>] {
        &self.parameter_tys
    }

    pub(crate) fn one_parameter_type(&self) -> Option<Type<'db>> {
        match self.parameter_types() {
            [ty] => Some(*ty),
            _ => None,
        }
    }

    pub(crate) fn two_parameter_types(&self) -> Option<(Type<'db>, Type<'db>)> {
        match self.parameter_types() {
            [first, second] => Some((*first, *second)),
            _ => None,
        }
    }

    pub(crate) fn three_parameter_types(&self) -> Option<(Type<'db>, Type<'db>, Type<'db>)> {
        match self.parameter_types() {
            [first, second, third] => Some((*first, *second, *third)),
            _ => None,
        }
    }

    fn report_diagnostics(
        &self,
        context: &InferContext<'db>,
        node: ast::AnyNodeRef,
        callable_ty: Type<'db>,
        callable_descriptor: Option<&CallableDescriptor>,
    ) {
        for error in &self.errors {
            error.report_diagnostic(context, node, callable_ty, callable_descriptor);
        }
    }

    pub(crate) fn has_binding_errors(&self) -> bool {
        !self.errors.is_empty()
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
pub(crate) enum CallBindingError<'db> {
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

impl<'db> CallBindingError<'db> {
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
        callable_descriptor: Option<&CallableDescriptor>,
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
                        if let Some(CallableDescriptor { kind, name }) = callable_descriptor {
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
                        if let Some(CallableDescriptor { kind, name }) = callable_descriptor {
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
                        if let Some(CallableDescriptor { kind, name }) = callable_descriptor {
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
                        if let Some(CallableDescriptor { kind, name }) = callable_descriptor {
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
                        if let Some(CallableDescriptor { kind, name }) = callable_descriptor {
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
