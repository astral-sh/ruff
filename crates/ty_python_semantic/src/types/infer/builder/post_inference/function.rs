use crate::{
    diagnostic::format_enumeration,
    types::{
        KnownInstanceType, Signature, Type, TypeVarKind,
        context::InferContext,
        diagnostic::{INVALID_LEGACY_POSITIONAL_PARAMETER, INVALID_TYPE_VARIABLE_DEFAULT},
        function::OverloadLiteral,
        infer_definition_types,
        signatures::ReturnCallableTypeVarScope,
        typevar::TypeVarInstance,
        visitor::find_over_type,
    },
};
use itertools::Itertools;
use ruff_db::{
    diagnostic::{Annotation, Span},
    parsed::parsed_module,
};
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange};
use ty_python_core::definition::Definition;

pub(crate) fn check_function_definition<'db>(
    context: &InferContext<'db, '_>,
    definition: Definition<'db>,
    file_expression_type: &impl Fn(&ast::Expr) -> Type<'db>,
) {
    let db = context.db();

    let Some(function_type) = infer_definition_types(db, definition).function_type(definition)
    else {
        return;
    };

    let last_definition = function_type.literal(db).last_definition;
    let signature = last_definition.raw_signature(db, ReturnCallableTypeVarScope::Public);

    check_legacy_positional_only_convention(context, last_definition, &signature);
    check_legacy_typevar_defaults(context, last_definition, &signature, file_expression_type);
    check_legacy_typevar_ordering(context, last_definition, &signature, file_expression_type);
}

/// Check for invalid applications of the pre-PEP-570 positional-only parameter convention.
fn check_legacy_positional_only_convention<'db>(
    context: &InferContext<'db, '_>,
    last_definition: OverloadLiteral<'db>,
    signature: &Signature<'db>,
) {
    let db = context.db();
    let node = last_definition.node(db, context.file(), context.module());
    let ast_parameters = &node.parameters;

    // If the function has any PEP-570 positional-only parameters,
    // assume that `__`-prefixed parameters are not meant to be positional-only
    if !ast_parameters.posonlyargs.is_empty() {
        return;
    }
    let parsed_parameters = signature.parameters();
    let mut previous_non_positional_only: Option<&ast::ParameterWithDefault> = None;

    for (param_node, param) in std::iter::zip(ast_parameters, parsed_parameters) {
        let ast::AnyParameterRef::NonVariadic(param_node) = param_node else {
            continue;
        };
        if param.is_positional_only() {
            continue;
        }

        // Valid uses of the PEP-484 positional-only convention will have been detected as such
        // in the first iteration over this scope, so `param.is_positional_only()` will return `true`
        // for those. We only get here for invalid uses of the PEP-484 positional-only convention.
        if param_node.uses_pep_484_positional_only_convention() {
            let Some(builder) =
                context.report_lint(&INVALID_LEGACY_POSITIONAL_PARAMETER, param_node.name())
            else {
                continue;
            };
            let mut diagnostic = builder.into_diagnostic(
                "Invalid use of the legacy convention \
                    for positional-only parameters",
            );
            diagnostic.set_primary_message(
                "Parameter name begins with `__` but will not be treated as positional-only",
            );
            diagnostic.info(
                "A parameter can only be positional-only \
                    if it precedes all positional-or-keyword parameters",
            );
            if let Some(earlier_node) = previous_non_positional_only {
                diagnostic.annotate(
                    context
                        .secondary(earlier_node.name())
                        .message("Prior parameter here was positional-or-keyword"),
                );
            }
        } else if previous_non_positional_only.is_none() {
            previous_non_positional_only = Some(param_node);
        }
    }
}

/// Check whether any legacy `TypeVar` used in a function signature has a default
/// that references an out-of-scope type variable.
///
/// This check mirrors the class-level check at `report_invalid_typevar_default_reference`,
/// but for function/method generic contexts.
fn check_legacy_typevar_defaults<'db>(
    context: &InferContext<'db, '_>,
    last_definition: OverloadLiteral<'db>,
    signature: &Signature<'db>,
    file_expression_type: &impl Fn(&ast::Expr) -> Type<'db>,
) {
    let db = context.db();

    let Some(generic_context) = signature.generic_context else {
        return;
    };

    let typevars = generic_context
        .variables(db)
        .map(|bound_tvar| bound_tvar.typevar(db));

    for (i, typevar) in typevars.clone().enumerate() {
        // Only check legacy TypeVars; PEP 695 type parameters are already validated
        // by `check_default_for_outer_scope_typevars` in the type parameter scope.
        if !matches!(
            typevar.kind(db),
            TypeVarKind::Legacy | TypeVarKind::Pep613Alias | TypeVarKind::ParamSpec
        ) {
            continue;
        }

        let Some(default_ty) = typevar.default_type(db) else {
            continue;
        };

        let first_bad_tvar = find_over_type(db, default_ty, false, |t| {
            let tvar = match t {
                Type::TypeVar(tvar) => tvar.typevar(db),
                Type::KnownInstance(KnownInstanceType::TypeVar(tvar)) => tvar,
                _ => return None,
            };
            if !typevars.clone().take(i).contains(&tvar) {
                Some(tvar)
            } else {
                None
            }
        });

        let Some(bad_typevar) = first_bad_tvar else {
            continue;
        };

        let is_later_in_list = typevars.clone().skip(i).contains(&bad_typevar);
        let node = last_definition.node(db, context.file(), context.module());

        let primary_range =
            find_typevar_annotation_range(context, node, typevar, file_expression_type);

        let Some(builder) = context.report_lint(&INVALID_TYPE_VARIABLE_DEFAULT, primary_range)
        else {
            continue;
        };
        let typevar_name = typevar.name(db);
        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Invalid use of type variable `{typevar_name}`",
        ));

        if is_later_in_list {
            diagnostic.set_primary_message(format_args!(
                "Default of `{typevar_name}` references later type parameter `{}`",
                bad_typevar.name(db),
            ));
            diagnostic.set_concise_message(format_args!(
                "Invalid use of type variable `{typevar_name}`: default of `{typevar_name}` \
                    refers to later parameter `{}`",
                bad_typevar.name(db)
            ));
        } else {
            diagnostic.set_primary_message(format_args!(
                "Default of `{typevar_name}` references out-of-scope type variable `{}`",
                bad_typevar.name(db),
            ));
            diagnostic.set_concise_message(format_args!(
                "Invalid use of type variable `{typevar_name}`: default of `{typevar_name}` \
                    refers to out-of-scope type variable `{}`",
                bad_typevar.name(db)
            ));
        }

        if let Some(typevar_definition) = typevar.definition(db) {
            let file = typevar_definition.file(db);
            diagnostic.annotate(
                Annotation::secondary(Span::from(
                    typevar_definition.full_range(db, &parsed_module(db, file).load(db)),
                ))
                .message(format_args!("`{typevar_name}` defined here")),
            );
        }

        diagnostic.info("See https://typing.python.org/en/latest/spec/generics.html#scoping-rules");
    }
}

fn find_typevar_annotation_range<'db>(
    context: &InferContext<'db, '_>,
    node: &ast::StmtFunctionDef,
    typevar: TypeVarInstance<'db>,
    file_expression_type: impl Fn(&ast::Expr) -> Type<'db>,
) -> TextRange {
    let db = context.db();
    let typevar_id = typevar.identity(db);

    node.parameters
        .iter()
        .filter_map(ast::AnyParameterRef::annotation)
        .chain(node.returns.as_deref())
        .find(|ann| file_expression_type(ann).references_typevar(db, typevar_id))
        .map(Ranged::range)
        .unwrap_or_else(|| node.name.range())
}

/// Check that legacy `TypeVar`s without defaults don't follow `TypeVar`s with defaults
/// in a function's generic context.
///
/// This mirrors the class-level check using `report_invalid_type_param_order`, but for
/// function/method generic contexts using the `invalid-type-variable-default` lint.
fn check_legacy_typevar_ordering<'db>(
    context: &InferContext<'db, '_>,
    last_definition: OverloadLiteral<'db>,
    signature: &Signature<'db>,
    file_expression_type: &impl Fn(&ast::Expr) -> Type<'db>,
) {
    struct State<'db> {
        typevar_with_default: TypeVarInstance<'db>,
        invalid_later_tvars: Vec<TypeVarInstance<'db>>,
    }

    let db = context.db();

    let Some(generic_context) = signature.generic_context else {
        return;
    };

    let mut state: Option<State<'db>> = None;

    for bound_typevar in generic_context.variables(db) {
        let typevar = bound_typevar.typevar(db);

        // Only check legacy TypeVars; PEP 695 ordering is validated by the parser.
        if !matches!(
            typevar.kind(db),
            TypeVarKind::Legacy | TypeVarKind::Pep613Alias | TypeVarKind::ParamSpec
        ) {
            continue;
        }

        let has_default = typevar.default_type(db).is_some();

        if let Some(state) = state.as_mut() {
            if !has_default {
                state.invalid_later_tvars.push(typevar);
            }
        } else if has_default {
            state = Some(State {
                typevar_with_default: typevar,
                invalid_later_tvars: vec![],
            });
        }
    }

    let Some(state) = state else {
        return;
    };

    if state.invalid_later_tvars.is_empty() {
        return;
    }

    let node = last_definition.node(db, context.file(), context.module());

    let primary_range = find_typevar_annotation_range(
        context,
        node,
        state.invalid_later_tvars[0],
        file_expression_type,
    );

    let Some(builder) = context.report_lint(&INVALID_TYPE_VARIABLE_DEFAULT, primary_range) else {
        return;
    };

    let mut diagnostic = builder.into_diagnostic(
        "Type parameters without defaults cannot follow type parameters with defaults",
    );

    let typevar_with_default_name = state.typevar_with_default.name(db);

    diagnostic.set_concise_message(format_args!(
        "Type parameter `{}` without a default cannot follow \
            earlier parameter `{typevar_with_default_name}` with a default",
        state.invalid_later_tvars[0].name(db),
    ));

    if let [single_typevar] = &*state.invalid_later_tvars {
        diagnostic.set_primary_message(format_args!(
            "Type variable `{}` does not have a default",
            single_typevar.name(db),
        ));
    } else {
        let later_typevars =
            format_enumeration(state.invalid_later_tvars.iter().map(|tv| tv.name(db)));
        diagnostic.set_primary_message(format_args!(
            "Type variables {later_typevars} do not have defaults",
        ));
    }

    let secondary_range = find_typevar_annotation_range(
        context,
        node,
        state.typevar_with_default,
        file_expression_type,
    );

    diagnostic.annotate(context.secondary(secondary_range).message(format_args!(
        "Earlier TypeVar `{typevar_with_default_name}` has a default"
    )));

    for tvar in [state.typevar_with_default, state.invalid_later_tvars[0]] {
        let Some(definition) = tvar.definition(db) else {
            continue;
        };
        let file = definition.file(db);
        diagnostic.annotate(
            Annotation::secondary(Span::from(
                definition.full_range(db, &parsed_module(db, file).load(db)),
            ))
            .message(format_args!("`{}` defined here", tvar.name(db))),
        );
    }
}
