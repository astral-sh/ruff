use ruff_python_ast as ast;
use ty_python_core::place::PlaceExpr;
use ty_python_core::place_table;
use ty_python_core::scope::ScopeId;

use crate::Db;
use crate::types::Type;
use crate::types::call::{Binding, Bindings};

pub(super) fn bind_type_guard_return_type<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    return_ty: Type<'db>,
    bindings: &Bindings<'db>,
    arguments: &ast::Arguments,
) -> Type<'db> {
    let narrowed_argument_index = || {
        bindings
            .single_element()
            .and_then(|binding| {
                binding
                    .signature_type
                    .as_function_literal()
                    .or_else(|| binding.callable_type.as_function_literal())
                    .map(|function| {
                        usize::from(
                            function.has_implicit_receiver(db) && binding.bound_type.is_none(),
                        )
                    })
            })
            .unwrap_or(0)
    };

    let find_narrowed_place = || {
        // Use the call binding to find the argument that maps to the first parameter a type
        // guard can narrow. This supports keyword arguments without falling back to a later
        // parameter when the target is defaulted.
        let matched_narrowed_argument_index = bindings.single_element().and_then(|binding| {
            let has_implicit_receiver = binding
                .signature_type
                .as_function_literal()
                .or_else(|| binding.callable_type.as_function_literal())
                .is_some_and(|function| function.has_implicit_receiver(db));
            let bound_argument_offset = usize::from(binding.bound_type.is_some());
            let narrowed_parameter_index =
                usize::from(bound_argument_offset > 0 || has_implicit_receiver);
            let narrowed_argument_index = |overload: &Binding<'db>| {
                overload
                    .argument_matches()
                    .iter()
                    .enumerate()
                    .skip(bound_argument_offset)
                    .find_map(|(argument_index, matched_argument)| {
                        matched_argument
                            .parameters
                            .iter()
                            .any(|parameter| parameter.index == narrowed_parameter_index)
                            .then_some(argument_index - bound_argument_offset)
                    })
            };
            let mut matching_overloads = binding.matching_overloads();
            let (_, first_overload) = matching_overloads.next()?;
            let first_argument_index = narrowed_argument_index(first_overload);

            Some(
                if matching_overloads
                    .all(|(_, overload)| narrowed_argument_index(overload) == first_argument_index)
                {
                    first_argument_index
                } else {
                    None
                },
            )
        });

        let argument = match matched_narrowed_argument_index {
            Some(Some(argument_index)) => arguments.iter_source_order().nth(argument_index),
            // The target parameter was omitted, so there is no expression to narrow.
            Some(None) => None,
            // Preserve positional behavior when there isn't a unique callable binding whose
            // parameter mapping we can use.
            None => arguments
                .args
                .get(narrowed_argument_index())
                .map(ast::ArgOrKeyword::from),
        }?;
        if argument.is_variadic() {
            return None;
        }

        let place_expr = PlaceExpr::try_from_expr(argument.value())?;
        place_table(db, scope).place_id(&place_expr)
    };

    match return_ty {
        Type::TypeIs(type_is) => match find_narrowed_place() {
            Some(place) => type_is.bind(db, scope, place),
            None => return_ty,
        },
        Type::TypeGuard(type_guard) => match find_narrowed_place() {
            Some(place) => type_guard.bind(db, scope, place),
            None => return_ty,
        },
        _ => return_ty,
    }
}
