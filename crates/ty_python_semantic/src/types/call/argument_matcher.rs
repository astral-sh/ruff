//! This module handles the logic for matching call-site arguments to
//! target function parameters.

use std::fmt;

use ruff_db::diagnostic::{Annotation, Diagnostic, Severity, SubDiagnostic};
use ruff_db::parsed::parsed_module;

use super::{Argument, CallArguments, InferContext};
use crate::db::Db;
use crate::types::diagnostic::{
    CALL_NON_CALLABLE, INVALID_ARGUMENT_TYPE, MISSING_ARGUMENT, PARAMETER_ALREADY_ASSIGNED,
    TOO_MANY_POSITIONAL_ARGUMENTS, UNKNOWN_ARGUMENT,
};
use crate::types::function::{FunctionType, OverloadLiteral};
use crate::types::generics::SpecializationError;
use crate::types::signatures::{Parameter, ParameterForm, Parameters};
use crate::types::{CallableBinding, Type};
use ruff_python_ast as ast;

/// Matches call arguments to function parameters.
pub(crate) struct ArgumentMatcher<'a, 'db> {
    parameters: &'a Parameters<'db>,
    argument_forms: &'a mut [Option<ParameterForm>],
    conflicting_forms: &'a mut [bool],
    errors: &'a mut Vec<MatchArgumentsError<'db>>,

    /// The parameter that each argument is matched with.
    argument_parameters: Vec<Option<usize>>,
    /// Whether each parameter has been matched with an argument.
    parameter_matched: Vec<bool>,
    next_positional: usize,
    first_excess_positional: Option<usize>,
    num_synthetic_args: usize,
}

impl<'a, 'db> ArgumentMatcher<'a, 'db> {
    /// Create a new argument matcher.
    pub(crate) fn new(
        arguments: &CallArguments,
        parameters: &'a Parameters<'db>,
        argument_forms: &'a mut [Option<ParameterForm>],
        conflicting_forms: &'a mut [bool],
        errors: &'a mut Vec<MatchArgumentsError<'db>>,
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

    /// Get the adjusted argument index (excluding synthetic arguments).
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

    /// Assign an argument to a parameter.
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
                self.errors
                    .push(MatchArgumentsError::ParameterAlreadyAssigned {
                        argument_index: self.get_argument_index(argument_index),
                        parameter: ParameterContext::new(parameter, parameter_index, positional),
                    });
            }
        }
        self.argument_parameters[argument_index] = Some(parameter_index);
        self.parameter_matched[parameter_index] = true;
    }

    /// Match a positional argument to a parameter.
    pub(crate) fn match_positional(
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

    /// Match a keyword argument to a parameter.
    pub(crate) fn match_keyword(
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
            self.errors.push(MatchArgumentsError::UnknownArgument {
                argument_name: ast::name::Name::new(name),
                argument_index: self.get_argument_index(argument_index),
            });
            return Err(());
        };
        self.assign_argument(argument_index, argument, parameter_index, parameter, false);
        Ok(())
    }

    /// Match all arguments to parameters using the provided matcher.
    /// This is a unified routine that can be used by both IDE support and binding logic.
    pub(crate) fn match_arguments(mut self, arguments: &CallArguments<'a>) -> Box<[Option<usize>]> {
        for (argument_index, argument) in arguments.iter().enumerate() {
            match argument {
                Argument::Positional | Argument::Synthetic => {
                    let _ = self.match_positional(argument_index, argument);
                }
                Argument::Keyword(name) => {
                    let _ = self.match_keyword(argument_index, argument, name);
                }
                Argument::Variadic | Argument::Keywords => {
                    // TODO: Handle variadic arguments
                    continue;
                }
            }
        }
        self.finish()
    }

    /// Finish matching and return the argument-to-parameter mapping.
    pub(crate) fn finish(self) -> Box<[Option<usize>]> {
        if let Some(first_excess_argument_index) = self.first_excess_positional {
            self.errors
                .push(MatchArgumentsError::TooManyPositionalArguments {
                    first_excess_argument_index: self
                        .get_argument_index(first_excess_argument_index),
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
            self.errors.push(MatchArgumentsError::MissingArguments {
                parameters: ParameterContexts(missing),
            });
        }

        self.argument_parameters.into_boxed_slice()
    }
}

/// Context information for a parameter in error reporting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParameterContext {
    pub(crate) name: Option<ast::name::Name>,
    pub(crate) index: usize,

    /// Was the argument for this parameter passed positionally, and matched to a non-variadic
    /// positional parameter? (If so, we will provide the index in the diagnostic, not just the
    /// name.)
    pub(crate) positional: bool,
}

impl ParameterContext {
    pub(crate) fn new(parameter: &Parameter, index: usize, positional: bool) -> Self {
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

/// A collection of parameter contexts for error reporting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParameterContexts(pub(crate) Vec<ParameterContext>);

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

/// Errors that can occur when matching arguments to parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MatchArgumentsError<'db> {
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

impl<'db> MatchArgumentsError<'db> {
    pub(crate) fn report_diagnostic(
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

    pub(crate) fn get_node(
        node: ast::AnyNodeRef,
        argument_index: Option<usize>,
    ) -> ast::AnyNodeRef {
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
pub(crate) struct UnionDiagnostic<'b, 'db> {
    /// The type of the union.
    pub(crate) callable_type: Type<'db>,
    /// The specific binding that failed.
    pub(crate) binding: &'b CallableBinding<'db>,
}

impl UnionDiagnostic<'_, '_> {
    /// Create a new union diagnostic.
    pub(crate) fn new<'b, 'db>(
        callable_type: Type<'db>,
        binding: &'b CallableBinding<'db>,
    ) -> UnionDiagnostic<'b, 'db> {
        UnionDiagnostic {
            callable_type,
            binding,
        }
    }

    /// Adds context about any relevant union function types to the given
    /// diagnostic.
    pub(crate) fn add_union_context(&self, db: &'_ dyn Db, diag: &mut Diagnostic) {
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
pub(crate) struct MatchingOverloadLiteral<'db> {
    /// The position of the matching overload in the list of overloads.
    pub(crate) index: usize,
    /// The kind of function this overload is for.
    pub(crate) kind: FunctionKind,
    /// The function literal that this overload belongs to.
    ///
    /// This is used to retrieve the overload at the given index.
    pub(crate) function: FunctionType<'db>,
}

impl<'db> MatchingOverloadLiteral<'db> {
    /// Create a new matching overload literal.
    pub(crate) fn new(index: usize, kind: FunctionKind, function: FunctionType<'db>) -> Self {
        Self {
            index,
            kind,
            function,
        }
    }

    /// Returns the [`OverloadLiteral`] representing this matching overload.
    pub(crate) fn get(&self, db: &'db dyn Db) -> Option<OverloadLiteral<'db>> {
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

pub(crate) struct CallableDescription {
    pub(crate) kind: FunctionKind,
    pub(crate) name: String,
}

impl CallableDescription {
    /// Create a new callable description.
    pub(crate) fn new(db: &dyn Db, callable_type: Type) -> Option<CallableDescription> {
        match callable_type {
            Type::FunctionLiteral(function) => Some(CallableDescription {
                kind: FunctionKind::Function,
                name: function.name(db).to_string(),
            }),
            Type::ClassLiteral(class_type) => Some(CallableDescription {
                kind: FunctionKind::Class,
                name: class_type.name(db).to_string(),
            }),
            Type::BoundMethod(bound_method) => Some(CallableDescription {
                kind: FunctionKind::BoundMethod,
                name: bound_method.function(db).name(db).to_string(),
            }),
            Type::MethodWrapper(crate::types::MethodWrapperKind::FunctionTypeDunderGet(
                function,
            )) => Some(CallableDescription {
                kind: FunctionKind::MethodWrapper,
                name: function.name(db).to_string(),
            }),
            Type::MethodWrapper(crate::types::MethodWrapperKind::PropertyDunderGet(_)) => {
                Some(CallableDescription {
                    kind: FunctionKind::MethodWrapper,
                    name: "`__get__` of property".to_string(),
                })
            }
            Type::WrapperDescriptor(kind) => Some(CallableDescription {
                kind: FunctionKind::WrapperDescriptor,
                name: match kind {
                    crate::types::WrapperDescriptorKind::FunctionTypeDunderGet => {
                        "FunctionType.__get__".to_string()
                    }
                    crate::types::WrapperDescriptorKind::PropertyDunderGet => {
                        "property.__get__".to_string()
                    }
                    crate::types::WrapperDescriptorKind::PropertyDunderSet => {
                        "property.__set__".to_string()
                    }
                },
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum FunctionKind {
    Function,
    BoundMethod,
    MethodWrapper,
    Class,
    WrapperDescriptor,
}

impl fmt::Display for FunctionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FunctionKind::Function => write!(f, "function"),
            FunctionKind::BoundMethod => write!(f, "bound method"),
            FunctionKind::MethodWrapper => write!(f, "method wrapper `__get__` of function"),
            FunctionKind::Class => write!(f, "class"),
            FunctionKind::WrapperDescriptor => write!(f, "wrapper descriptor"),
        }
    }
}

// When the number of unmatched overloads exceeds this number, we stop printing them to avoid
// excessive output.
//
// An example of a routine with many many overloads:
// https://github.com/henribru/google-api-python-client-stubs/blob/master/googleapiclient-stubs/discovery.pyi
pub(crate) const MAXIMUM_OVERLOADS: usize = 50;
