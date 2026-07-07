use itertools::Itertools;

use crate::Db;
use crate::types::enums::enum_member_literals;
use crate::types::tuple::Tuple;
use crate::types::{KnownClass, Type};

/// Maximum number of expanded types that can be generated from a single tuple's
/// Cartesian product in [`expand_type`].
///
/// See: [pyright's `maxSingleOverloadArgTypeExpansionCount`][pyright]
///
/// [pyright]: https://github.com/microsoft/pyright/blob/5a325e4874e775436671eed65ad696787a1ef74b/packages/pyright-internal/src/analyzer/typeEvaluator.ts#L570
const MAX_TUPLE_EXPANSION: usize = 64;

/// Expands a type into its possible subtypes, if applicable.
///
/// Returns [`None`] if the type cannot be expanded.
pub(crate) fn expand_type<'db>(
    db: &'db dyn Db,
    program: crate::Program,
    ty: Type<'db>,
) -> Option<Vec<Type<'db>>> {
    match ty {
        Type::EnumComplement(complement) => Some(complement.remaining_literal_types(db)),
        Type::Intersection(intersection) => intersection.finite_alternatives(db, program),
        Type::NominalInstance(instance) => {
            let class = instance.class(db, program);

            if class.is_known(db, KnownClass::Bool) {
                return Some(vec![Type::bool_literal(true), Type::bool_literal(false)]);
            }

            // If the class is a fixed-length tuple subtype, we expand it to its elements.
            if let Some(spec) = instance.tuple_spec(db, program) {
                return match &*spec {
                    Tuple::Fixed(fixed_length_tuple) => {
                        // Pre-expand each element and compute the total Cartesian product size.
                        // Bail out early if the product would exceed `MAX_TUPLE_EXPANSION` to
                        // avoid exponential blowup (e.g. a 37-element tuple with 2-element
                        // unions would produce 2^37 types).
                        let per_element: Vec<_> = fixed_length_tuple
                            .iter_all_elements()
                            .map(|element| {
                                expand_type(db, program, element).unwrap_or_else(|| vec![element])
                            })
                            .collect();

                        let product_size: usize = per_element
                            .iter()
                            .try_fold(1usize, |acc, v| acc.checked_mul(v.len()))
                            .unwrap_or(usize::MAX);

                        if product_size <= 1 || product_size > MAX_TUPLE_EXPANSION {
                            None
                        } else {
                            let expanded = per_element
                                .into_iter()
                                .multi_cartesian_product()
                                .map(|types| Type::heterogeneous_tuple(db, types))
                                .collect::<Vec<_>>();
                            Some(expanded)
                        }
                    }
                    Tuple::Variable(_) => None,
                };
            }

            if let Some(enum_members) = enum_member_literals(db, class.class_literal(db), None) {
                return Some(enum_members.collect());
            }

            None
        }
        Type::Union(union) => Some(
            union
                .elements(db)
                .iter()
                .flat_map(|element| match element {
                    Type::EnumComplement(complement) => complement.remaining_literal_types(db),
                    Type::Intersection(intersection) => intersection
                        .finite_alternatives(db, program)
                        .unwrap_or_else(|| vec![*element]),
                    _ => vec![*element],
                })
                .collect(),
        ),
        // For type aliases, expand the underlying value type.
        Type::TypeAlias(alias) => expand_type(db, program, alias.value_type(db)),
        // We don't handle `type[A | B]` here because it's already stored in the expanded form
        // i.e., `type[A] | type[B]` which is handled by the `Type::Union` case.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::db::tests::setup_db;
    use crate::types::tuple::TupleType;
    use crate::types::{KnownClass, Type, UnionType};

    use super::expand_type;

    #[test]
    fn expand_union_type() {
        let db = setup_db();
        let program = db.program();
        let types = [
            KnownClass::Int.to_instance(&db, program),
            KnownClass::Str.to_instance(&db, program),
            KnownClass::Bytes.to_instance(&db, program),
        ];
        let union_type = UnionType::from_elements(&db, program, types);
        let expanded = expand_type(&db, program, union_type).unwrap();
        assert_eq!(expanded.len(), types.len());
        assert_eq!(expanded, types);
    }

    #[test]
    fn expand_bool_type() {
        let db = setup_db();
        let program = db.program();
        let bool_instance = KnownClass::Bool.to_instance(&db, program);
        let expanded = expand_type(&db, program, bool_instance).unwrap();
        let expected_types = [Type::bool_literal(true), Type::bool_literal(false)];
        assert_eq!(expanded.len(), expected_types.len());
        assert_eq!(expanded, expected_types);
    }

    #[test]
    fn expand_tuple_type() {
        let db = setup_db();
        let program = db.program();

        let int_ty = KnownClass::Int.to_instance(&db, program);
        let str_ty = KnownClass::Str.to_instance(&db, program);
        let bytes_ty = KnownClass::Bytes.to_instance(&db, program);
        let bool_ty = KnownClass::Bool.to_instance(&db, program);
        let true_ty = Type::bool_literal(true);
        let false_ty = Type::bool_literal(false);

        // Empty tuple
        let empty_tuple = Type::empty_tuple(&db);
        let expanded = expand_type(&db, program, empty_tuple);
        assert!(expanded.is_none());

        // None of the elements can be expanded.
        let tuple_type1 = Type::heterogeneous_tuple(&db, [int_ty, str_ty]);
        let expanded = expand_type(&db, program, tuple_type1);
        assert!(expanded.is_none());

        // All elements can be expanded.
        let tuple_type2 = Type::heterogeneous_tuple(
            &db,
            [
                bool_ty,
                UnionType::from_elements(&db, program, [int_ty, str_ty, bytes_ty]),
            ],
        );
        let expected_types = [
            Type::heterogeneous_tuple(&db, [true_ty, int_ty]),
            Type::heterogeneous_tuple(&db, [true_ty, str_ty]),
            Type::heterogeneous_tuple(&db, [true_ty, bytes_ty]),
            Type::heterogeneous_tuple(&db, [false_ty, int_ty]),
            Type::heterogeneous_tuple(&db, [false_ty, str_ty]),
            Type::heterogeneous_tuple(&db, [false_ty, bytes_ty]),
        ];
        let expanded = expand_type(&db, program, tuple_type2).unwrap();
        assert_eq!(expanded, expected_types);

        // Mixed set of elements where some can be expanded while others cannot be.
        let tuple_type3 = Type::heterogeneous_tuple(
            &db,
            [
                bool_ty,
                int_ty,
                UnionType::from_elements(&db, program, [str_ty, bytes_ty]),
                str_ty,
            ],
        );
        let expected_types = [
            Type::heterogeneous_tuple(&db, [true_ty, int_ty, str_ty, str_ty]),
            Type::heterogeneous_tuple(&db, [true_ty, int_ty, bytes_ty, str_ty]),
            Type::heterogeneous_tuple(&db, [false_ty, int_ty, str_ty, str_ty]),
            Type::heterogeneous_tuple(&db, [false_ty, int_ty, bytes_ty, str_ty]),
        ];
        let expanded = expand_type(&db, program, tuple_type3).unwrap();
        assert_eq!(expanded, expected_types);

        // Variable-length tuples are not expanded.
        let variable_length_tuple = Type::tuple(TupleType::mixed(
            &db,
            [bool_ty],
            int_ty,
            [
                UnionType::from_elements(&db, program, [str_ty, bytes_ty]),
                str_ty,
            ],
        ));
        let expanded = expand_type(&db, program, variable_length_tuple);
        assert!(expanded.is_none());
    }
}
