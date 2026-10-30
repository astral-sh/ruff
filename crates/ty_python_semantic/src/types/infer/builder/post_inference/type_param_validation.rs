use ruff_python_ast::{self as ast, name::Name};
use ruff_text_size::{Ranged, TextRange};

use crate::diagnostic::format_enumeration;
use crate::types::{
    context::InferContext,
    diagnostic::{INVALID_TYPE_FORM, INVALID_TYPE_VARIABLE_DEFAULT},
};

#[derive(Clone, Copy)]
pub(crate) enum TypeParameterOwner<'a> {
    GenericClass(&'a Name),
    TypeAlias(&'a Name),
}

/// Check that a PEP 695 class or type alias parameter list contains at most one `TypeVarTuple`.
///
/// Classes and type aliases can be explicitly specialized, so multiple `TypeVarTuple`s would make
/// it ambiguous which pack consumes each type argument. Generic functions cannot be explicitly
/// specialized and intentionally do not use this validation.
pub(crate) fn check_single_typevar_tuple_pep695(
    context: &InferContext<'_, '_>,
    type_params: &ast::TypeParams,
    owner: TypeParameterOwner<'_>,
) {
    let (owner_kind, owner_name) = match owner {
        TypeParameterOwner::GenericClass(name) => ("Generic class", name),
        TypeParameterOwner::TypeAlias(name) => ("Type alias", name),
    };
    let mut first_typevar_tuple: Option<&ast::TypeParamTypeVarTuple> = None;

    for type_param in type_params {
        let ast::TypeParam::TypeVarTuple(typevar_tuple) = type_param else {
            continue;
        };

        let Some(first_typevar_tuple) = first_typevar_tuple else {
            first_typevar_tuple = Some(typevar_tuple);
            continue;
        };

        let Some(builder) = context.report_lint(&INVALID_TYPE_FORM, typevar_tuple) else {
            return;
        };

        let mut diagnostic = builder.into_diagnostic(format_args!(
            "{owner_kind} `{owner_name}` cannot have multiple `TypeVarTuple` type parameters"
        ));

        diagnostic.set_primary_annotation_message(format_args!(
            "`{}` is an additional TypeVarTuple",
            typevar_tuple.name
        ));

        diagnostic.annotate(context.secondary(first_typevar_tuple).message(format_args!(
            "`{}` is the first TypeVarTuple",
            first_typevar_tuple.name
        )));

        diagnostic.info(
            "See https://typing.python.org/en/latest/spec/generics.html#multiple-type-variable-tuples-not-allowed",
        );

        return;
    }
}

pub(crate) fn check_no_default_after_typevar_tuple_pep695(
    context: &InferContext<'_, '_>,
    type_params: &ast::TypeParams,
) {
    check_no_default_after_typevar_tuple(
        context,
        type_params.iter(),
        |param| {
            param
                .as_type_var_tuple()
                .map(|pack| (&pack.name.id, pack.range()))
        },
        |param| {
            let typevar = param.as_type_var()?;
            typevar.default.as_ref()?;
            Some((&typevar.name.id, typevar.range()))
        },
    );
}

/// A defaulted `TypeVar` anywhere after a pack makes it ambiguous how many arguments
/// belong to the pack. A `ParamSpec` can follow it because its argument list is distinct.
pub(in crate::types::infer::builder) fn check_no_default_after_typevar_tuple<'a, P>(
    context: &InferContext<'_, '_>,
    mut parameters: impl Iterator<Item = P>,
    as_pack: impl FnMut(P) -> Option<(&'a Name, TextRange)>,
    as_defaulted_typevar: impl FnMut(P) -> Option<(&'a Name, TextRange)>,
) {
    if !context.is_lint_enabled(&INVALID_TYPE_VARIABLE_DEFAULT) {
        return;
    }

    let Some((pack_name, pack_range)) = parameters.find_map(as_pack) else {
        return;
    };
    let defaults: Vec<_> = parameters.filter_map(as_defaulted_typevar).collect();
    let Some((&(first_name, first_range), others)) = defaults.split_first() else {
        return;
    };
    let Some(builder) = context.report_lint(&INVALID_TYPE_VARIABLE_DEFAULT, first_range) else {
        return;
    };

    let mut diagnostic =
        builder.into_diagnostic("TypeVars with defaults cannot follow a TypeVarTuple");
    diagnostic.set_primary_annotation_message(format_args!("`{first_name}` has a default"));

    if others.is_empty() {
        diagnostic.set_concise_message(format_args!(
            "Type parameter `{first_name}` with a default follows TypeVarTuple `{pack_name}`"
        ));
    } else {
        let names = format_enumeration(defaults.iter().map(|(name, _)| name));
        diagnostic.set_concise_message(format_args!(
            "Type parameters {names} with defaults follow TypeVarTuple `{pack_name}`"
        ));
        for (name, range) in others {
            diagnostic.annotate(
                context
                    .secondary(*range)
                    .message(format_args!("`{name}` also has a default")),
            );
        }
    }

    diagnostic.annotate(
        context
            .secondary(pack_range)
            .message(format_args!("`{pack_name}` is a TypeVarTuple")),
    );
    diagnostic.info(
        "Using the default would let the TypeVarTuple consume an extra positional type argument",
    );
}
