use super::{Argument, CallArguments, InferContext, Signature, Type};
use crate::db::Db;
use crate::types::diagnostic::{
    INVALID_ARGUMENT_TYPE, MISSING_ARGUMENT, PARAMETER_ALREADY_ASSIGNED,
    TOO_MANY_POSITIONAL_ARGUMENTS, UNKNOWN_ARGUMENT,
};
use crate::types::UnionType;
use ruff_python_ast as ast;

/// Bind a [`CallArguments`] against a callable [`Signature`].
///
/// The returned [`CallBinding`] provides the return type of the call, the bound types for all
/// parameters, and any errors resulting from binding the call.
pub(crate) fn bind_call<'db>(
    db: &'db dyn Db,
    arguments: &CallArguments<'db>,
    signature: &Signature<'db>,
) -> CallBinding<'db> {
    let param_count = signature.parameter_count();
    let mut parameter_tys = vec![None; param_count];
    let mut errors = vec![];
    let mut next_positional = 0;
    let mut first_excess_positional = None;
    for (argument_index, argument) in arguments.iter().enumerate() {
        let (index, parameter, argument_ty) = match argument {
            Argument::Positional(ty) => {
                let Some((index, parameter)) = signature
                    .positional_at_index(next_positional)
                    .map(|param| (next_positional, param))
                    .or_else(|| signature.variadic_parameter())
                else {
                    first_excess_positional.get_or_insert(argument_index);
                    next_positional += 1;
                    continue;
                };
                next_positional += 1;
                (index, parameter, ty)
            }
            Argument::Keyword { name, ty } => {
                let Some((index, parameter)) = signature
                    .keyword_by_name(name)
                    .or_else(|| signature.keywords_parameter())
                else {
                    errors.push(CallBindingError::UnknownArgument {
                        unknown_name: name.clone(),
                        unknown_argument_index: argument_index,
                    });
                    continue;
                };
                (index, parameter, ty)
            }

            Argument::Variadic(_) | Argument::Keywords(_) => {
                // TODO
                continue;
            }
        };
        let expected_ty = parameter.annotated_ty();
        if !argument_ty.is_assignable_to(db, expected_ty) {
            errors.push(CallBindingError::InvalidArgumentType {
                parameter_name: parameter.display_name(index),
                argument_index,
                expected_ty,
                provided_ty: *argument_ty,
            });
        }
        if let Some(existing) = parameter_tys[index].replace(*argument_ty) {
            if parameter.is_variadic() {
                let union = UnionType::from_elements(db, [existing, *argument_ty]);
                parameter_tys[index].replace(union);
            } else {
                errors.push(CallBindingError::ParameterAlreadyAssigned {
                    argument_index,
                    parameter_name: parameter.display_name(index),
                });
            }
        }
    }
    if let Some(first_excess_argument_index) = first_excess_positional {
        errors.push(CallBindingError::TooManyPositionalArguments {
            first_excess_argument_index,
            expected_positional_count: signature.positional_parameter_count(),
            provided_positional_count: next_positional,
        });
    }
    for (index, bound_ty) in parameter_tys.iter().enumerate() {
        if bound_ty.is_none() {
            let param = signature
                .parameter_at_index(index)
                .expect("parameter_tys array should not be larger than number of parameters");
            if param.is_variadic() || param.is_keywords() || param.default_ty().is_some() {
                // variadic/keywords and defaulted arguments are not required
                continue;
            }
            errors.push(CallBindingError::MissingArgument {
                parameter_name: param.display_name(index),
            });
        }
    }

    CallBinding {
        return_ty: signature.return_ty,
        parameter_tys: parameter_tys
            .into_iter()
            .map(|opt_ty| opt_ty.unwrap_or(Type::Unknown))
            .collect(),
        errors,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CallBinding<'db> {
    /// Return type of the call.
    return_ty: Type<'db>,

    /// Bound types for parameters, in parameter source order.
    parameter_tys: Box<[Type<'db>]>,

    /// Call binding errors, if any.
    // TODO use SmallVec once variance bug is fixed
    errors: Vec<CallBindingError<'db>>,
}

impl<'db> CallBinding<'db> {
    // TODO remove this constructor and construct always from `bind_call`
    pub(crate) fn from_return_ty(return_ty: Type<'db>) -> Self {
        Self {
            return_ty,
            parameter_tys: Box::default(),
            errors: vec![],
        }
    }

    pub(crate) fn set_return_ty(&mut self, return_ty: Type<'db>) {
        self.return_ty = return_ty;
    }

    pub(crate) fn return_ty(&self) -> Type<'db> {
        self.return_ty
    }

    pub(crate) fn parameter_tys(&self) -> &[Type<'db>] {
        &self.parameter_tys
    }

    pub(crate) fn first_parameter(&self) -> Option<Type<'db>> {
        self.parameter_tys().first().copied()
    }

    pub(super) fn report_diagnostics(&self, context: &InferContext<'db>, node: ast::AnyNodeRef) {
        for error in &self.errors {
            error.report_diagnostic(context, node);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CallBindingError<'db> {
    /// The type of an argument is not assignable to the annotated type of its corresponding
    /// parameter.
    InvalidArgumentType {
        parameter_name: ast::name::Name,
        argument_index: usize,
        expected_ty: Type<'db>,
        provided_ty: Type<'db>,
    },
    /// A required parameter (that is, one without a default) is not supplied by any argument.
    MissingArgument { parameter_name: ast::name::Name },
    /// A call argument can't be matched to any parameter.
    UnknownArgument {
        unknown_name: ast::name::Name,
        unknown_argument_index: usize,
    },
    /// More positional arguments are provided in the call than can be handled by the signature.
    TooManyPositionalArguments {
        first_excess_argument_index: usize,
        expected_positional_count: usize,
        provided_positional_count: usize,
    },
    /// Multiple arguments were provided for a single parameter.
    ParameterAlreadyAssigned {
        argument_index: usize,
        parameter_name: ast::name::Name,
    },
}

impl<'db> CallBindingError<'db> {
    pub(super) fn report_diagnostic(&self, context: &InferContext<'db>, node: ast::AnyNodeRef) {
        match self {
            Self::InvalidArgumentType {
                parameter_name,
                argument_index,
                expected_ty,
                provided_ty,
            } => {
                let provided_ty_display = provided_ty.display(context.db());
                let expected_ty_display = expected_ty.display(context.db());
                context.report_lint(
                    &INVALID_ARGUMENT_TYPE,
                    Self::get_node(node, *argument_index),
                    format_args!(
                        "Cannot assign type `{provided_ty_display}` to parameter \
                        `{parameter_name}` of type `{expected_ty_display}`",
                    ),
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
                        "Too many positional arguments: expected \
                        {expected_positional_count}, got {provided_positional_count}"
                    ),
                );
            }

            Self::MissingArgument { parameter_name } => {
                context.report_lint(
                    &MISSING_ARGUMENT,
                    node,
                    format_args!("No argument provided for required parameter `{parameter_name}`"),
                );
            }

            Self::UnknownArgument {
                unknown_name,
                unknown_argument_index,
            } => {
                context.report_lint(
                    &UNKNOWN_ARGUMENT,
                    Self::get_node(node, *unknown_argument_index),
                    format_args!("Argument `{unknown_name}` does not match any known parameter"),
                );
            }

            Self::ParameterAlreadyAssigned {
                argument_index,
                parameter_name,
            } => {
                context.report_lint(
                    &PARAMETER_ALREADY_ASSIGNED,
                    Self::get_node(node, *argument_index),
                    format_args!("Parameter `{parameter_name}` is already assigned"),
                );
            }
        }
    }

    fn get_node(node: ast::AnyNodeRef, argument_index: usize) -> ast::AnyNodeRef {
        // If we have a Call node, report the diagnostic on the correct argument node;
        // otherwise, report it on the entire provided node.
        match node {
            ast::AnyNodeRef::ExprCall(call_node) => {
                match call_node
                    .arguments
                    .arguments_source_order()
                    .nth(argument_index)
                    .expect("InvalidArgumentType argument_index should not be out of range")
                {
                    ast::ArgOrKeyword::Arg(expr) => expr.into(),
                    ast::ArgOrKeyword::Keyword(keyword) => keyword.into(),
                }
            }
            _ => node,
        }
    }
}
