use super::{Argument, CallArguments, InferContext, Type};
use crate::db::Db;
use crate::types::diagnostic::{
    INVALID_ARGUMENT_TYPE, MISSING_ARGUMENT, PARAMETER_ALREADY_ASSIGNED,
    TOO_MANY_POSITIONAL_ARGUMENTS, UNKNOWN_ARGUMENT,
};
use crate::types::signatures::{FormalParameter, SignatureShape, SignatureTypes};
use crate::types::{todo_type, CallableType, UnionType};
use ruff_db::diagnostic::{OldSecondaryDiagnosticMessage, Span};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

pub(crate) fn match_call_parameters<'db>(
    db: &'db dyn Db,
    arguments: &CallArguments<'_, 'db>,
    shape: &SignatureShape<'db>,
    callable_ty: Type<'db>,
) -> MatchedParameters<'db> {
    // The type assigned to each parameter at this call site.
    let mut arguments = vec![MatchedParameter::default(); arguments.len()];
    let mut matched_formals = vec![false; shape.len()];
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
        if matches!(argument, Argument::Synthetic(_)) {
            num_synthetic_args += 1;
        }
        let adjusted_argument_index = get_argument_index(argument_index, num_synthetic_args);
        let (formal_index, parameter, positional) = match argument {
            Argument::Positional(_) | Argument::Synthetic(_) => {
                let Some((index, parameter)) = shape
                    .get_positional(next_positional)
                    .map(|param| (next_positional, param))
                    .or_else(|| shape.variadic())
                else {
                    first_excess_positional.get_or_insert(argument_index);
                    next_positional += 1;
                    continue;
                };
                next_positional += 1;
                (index, parameter, !parameter.is_variadic())
            }
            Argument::Keyword { name, .. } => {
                let Some((index, parameter)) = shape
                    .keyword_by_name(name)
                    .or_else(|| shape.keyword_variadic())
                else {
                    errors.push(MatchParametersError::UnknownArgument {
                        argument_name: ast::name::Name::new(name),
                        argument_index: adjusted_argument_index,
                    });
                    continue;
                };
                (index, parameter, false)
            }

            Argument::Variadic(_) | Argument::Keywords(_) => {
                // TODO
                continue;
            }
        };
        arguments[argument_index].formal_index = Some(formal_index);
        arguments[argument_index].adjusted_argument_index = adjusted_argument_index;
        if let Some(existing) = matched_formals[formal_index].replace(true) {
            // It's fine (expected, even?) for multiple arguments to match a variadic parameter.
            if !parameter.is_variadic() && !parameter.is_keyword_variadic() {
                errors.push(MatchParametersError::ParameterAlreadyAssigned {
                    argument_index: adjusted_argument_index,
                    parameter: ParameterContext::new(parameter, formal_index, positional),
                });
            }
        }
    }

    if let Some(first_excess_argument_index) = first_excess_positional {
        errors.push(MatchParametersError::TooManyPositionalArguments {
            first_excess_argument_index: get_argument_index(
                first_excess_argument_index,
                num_synthetic_args,
            ),
            expected_positional_count: shape.positional().count(),
            provided_positional_count: next_positional,
        });
    }

    let mut missing = vec![];
    for (index, param) in shape.iter().enumerate() {
        if !matched_formals[index] {
            if param.is_variadic() || param.is_keyword_variadic() || param.default_type().is_some()
            {
                // variadic/keywords and defaulted arguments are not required
                continue;
            }
            missing.push(ParameterContext::new(param, index, false));
        }
    }
    if !missing.is_empty() {
        errors.push(MatchParametersError::MissingArguments {
            parameters: ParameterContexts(missing),
        });
    }

    MatchedParameters {
        callable_ty,
        arguments: arguments.into_boxed_slice(),
        errors,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MatchedParameters<'db> {
    /// Type of the callable object (function, class...)
    callable_ty: Type<'db>,

    /// Infomration about which formal parameter each actual argument was matched against. (This
    /// has the same length as the argument list.)
    arguments: Box<[MatchedParameter]>,

    /// Call binding errors, if any.
    errors: Vec<MatchedParametersError>,
}

impl<'db> MatchedParameters<'db> {
    pub(crate) fn callable_type(&self) -> Type<'db> {
        self.callable_ty
    }

    pub(crate) fn report_diagnostics(&self, context: &InferContext<'db>, node: ast::AnyNodeRef) {
        let callable_descriptor = self.callable_descriptor(context.db());
        for error in &self.errors {
            error.report_diagnostic(
                context,
                node,
                self.callable_ty,
                callable_descriptor.as_ref(),
            );
        }
    }

    pub(crate) fn has_match_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct MatchedParameter {
    /// The formal parameter index that this actual argument was matched against.
    formal_index: Option<usize>,
    /// The argument index adjusted for any synthetic parameters. which don't appear at the call
    /// site and thus won't be in the Call node arguments list.
    adjusted_argument_index: Option<usize>,
}

/// Bind a [`CallArguments`] against a callable [`Signature`].
///
/// The returned [`CallBinding`] provides the return type of the call, the bound types for all
/// parameters, and any errors resulting from binding the call.
pub(crate) fn bind_call<'db>(
    db: &'db dyn Db,
    arguments: &CallArguments<'_, 'db>,
    shape: &SignatureShape<'db>,
    types: &SignatureTypes<'db>,
    matched_params: &MatchedParameters<'db>,
) -> CallBinding<'db> {
    // The type assigned to each parameter at this call site.
    let mut parameter_tys = vec![Type::unknown(); arguments.len()];
    let mut errors = vec![];

    for (argument_index, argument) in arguments.iter().enumerate() {
        let Some(formal_index) = matched_params.formal_for_actual[argument_index] else {
            continue;
        };
        let matched_param = &matched_params.arguments[argument_index];
        let parameter = &shape[*formal_index];
        let parameter_types = &types[*formal_index];
        if let Some(expected_ty) = parameter_types.annotated_type() {
            let argument_ty = argument.ty();
            if !argument_ty.is_assignable_to(db, expected_ty) {
                errors.push(CallBindingError::InvalidArgumentType {
                    parameter: ParameterContext::new(parameter, *formal_index),
                    argument_index: matched_param.adjusted_argument_index,
                    expected_ty,
                    provided_ty: argument_ty,
                });
            }
            parameter_tys[argument_index] = argument_ty;
        }
    }

    CallBinding {
        callable_ty: matched_params.callable_ty,
        return_ty: types.return_ty.unwrap_or(Type::unknown()),
        parameter_tys: parameter_tys.into_boxed_slice(),
        errors,
    }
}

/// Describes a callable for the purposes of diagnostics.
#[derive(Debug)]
pub(crate) struct CallableDescriptor<'a> {
    name: &'a str,
    kind: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CallBinding<'db> {
    /// Type of the callable object (function, class...)
    callable_ty: Type<'db>,

    /// Return type of the call.
    return_ty: Type<'db>,

    /// Bound types for parameters, in parameter source order.
    parameter_tys: Box<[Type<'db>]>,

    /// Call binding errors, if any.
    errors: Vec<CallBindingError<'db>>,
}

impl<'db> CallBinding<'db> {
    // TODO remove this constructor and construct always from `bind_call`
    pub(crate) fn from_return_type(return_ty: Type<'db>) -> Self {
        Self {
            callable_ty: todo_type!("CallBinding::from_return_type"),
            return_ty,
            parameter_tys: Box::default(),
            errors: vec![],
        }
    }

    pub(crate) fn callable_type(&self) -> Type<'db> {
        self.callable_ty
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

    pub(crate) fn report_diagnostics(&self, context: &InferContext<'db>, node: ast::AnyNodeRef) {
        let callable_descriptor = self.callable_descriptor(context.db());
        for error in &self.errors {
            error.report_diagnostic(
                context,
                node,
                self.callable_ty,
                callable_descriptor.as_ref(),
            );
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
    fn new(parameter: &FormalParameter, index: usize) -> Self {
        Self {
            name: parameter.display_name(),
            index,
            positional: parameter.is_positional(),
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

fn get_argument_node(node: ast::AnyNodeRef, argument_index: Option<usize>) -> ast::AnyNodeRef {
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MatchedParametersError {
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

impl MatchedParametersError {
    pub(super) fn report_diagnostic<'db>(
        &self,
        context: &InferContext<'db>,
        node: ast::AnyNodeRef,
        callable_ty: Type<'db>,
        callable_descriptor: Option<&CallableDescriptor>,
    ) {
        match self {
            Self::TooManyPositionalArguments {
                first_excess_argument_index,
                expected_positional_count,
                provided_positional_count,
            } => {
                context.report_lint(
                    &TOO_MANY_POSITIONAL_ARGUMENTS,
                    get_argument_node(node, *first_excess_argument_index),
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
                    get_argument_node(node, *argument_index),
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
                    get_argument_node(node, *argument_index),
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
                    get_argument_node(node, *argument_index),
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
        }
    }
}
