use crate::types::{Class, IntersectionBuilder, Type};
use crate::Db;

use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum TypeApiError {
    #[error("Wrong number of arguments")]
    WrongArity,
    #[error("Unknown API expression")]
    UnknownApiExpression,
    #[error("Failed assertion")]
    FailedAssertion,
}

type Result<T> = std::result::Result<T, TypeApiError>;

fn expect_one_argument<'db>(mut arguments: impl Iterator<Item = Type<'db>>) -> Result<Type<'db>> {
    let first = arguments.next().ok_or(TypeApiError::WrongArity)?;
    if arguments.next().is_some() {
        return Err(TypeApiError::WrongArity);
    }
    Ok(first)
}

fn expect_two_arguments<'db>(
    mut arguments: impl Iterator<Item = Type<'db>>,
) -> Result<(Type<'db>, Type<'db>)> {
    let first = arguments.next().ok_or(TypeApiError::WrongArity)?;
    let second = arguments.next().ok_or(TypeApiError::WrongArity)?;
    if arguments.next().is_some() {
        return Err(TypeApiError::WrongArity);
    }
    Ok((first, second))
}

pub(crate) fn resolve_type_operation<'db>(
    db: &'db dyn Db,
    class: Class<'db>,
    arguments: impl Iterator<Item = Type<'db>>,
) -> Result<Type<'db>> {
    match class.name(db).as_str() {
        "Negate" => {
            let ty = expect_one_argument(arguments)?;
            Ok(ty.negate(db))
        }
        "Intersection" => {
            let intersection_ty = arguments
                .fold(IntersectionBuilder::new(db), |builder, ty| {
                    builder.add_positive(ty)
                })
                .build();
            Ok(intersection_ty)
        }
        "TypeOf" => {
            let ty = expect_one_argument(arguments)?;
            Ok(ty)
        }
        _ => Err(TypeApiError::UnknownApiExpression),
    }
}

pub(crate) fn resolve_type_predicate<'db>(
    db: &'db dyn Db,
    function: &str,
    arguments: impl Iterator<Item = Type<'db>>,
) -> Result<Type<'db>> {
    match function {
        // Predicates on types
        "is_equivalent_to" => {
            let (ty_a, ty_b) = expect_two_arguments(arguments)?;
            Ok(Type::BooleanLiteral(ty_a.is_equivalent_to(db, ty_b)))
        }
        "is_subtype_of" => {
            let (ty_a, ty_b) = expect_two_arguments(arguments)?;
            Ok(Type::BooleanLiteral(ty_a.is_subtype_of(db, ty_b)))
        }
        "is_assignable_to" => {
            let (ty_a, ty_b) = expect_two_arguments(arguments)?;
            Ok(Type::BooleanLiteral(ty_a.is_assignable_to(db, ty_b)))
        }
        "is_disjoint_from" => {
            let (ty_a, ty_b) = expect_two_arguments(arguments)?;
            Ok(Type::BooleanLiteral(ty_a.is_disjoint_from(db, ty_b)))
        }
        "is_fully_static" => {
            let ty = expect_one_argument(arguments)?;
            Ok(Type::BooleanLiteral(ty.is_fully_static(db)))
        }
        "is_singleton" => {
            let ty = expect_one_argument(arguments)?;
            Ok(Type::BooleanLiteral(ty.is_singleton(db)))
        }
        "is_single_valued" => {
            let ty = expect_one_argument(arguments)?;
            Ok(Type::BooleanLiteral(ty.is_single_valued(db)))
        }

        // Special operations
        "assert_true" => {
            let ty = expect_one_argument(arguments)?;
            if ty == Type::BooleanLiteral(true) {
                Ok(Type::none(db))
            } else {
                Err(TypeApiError::FailedAssertion)
            }
        }
        "assert_false" => {
            let ty = expect_one_argument(arguments)?;
            if ty == Type::BooleanLiteral(false) {
                Ok(Type::none(db))
            } else {
                Err(TypeApiError::FailedAssertion)
            }
        }

        _ => Err(TypeApiError::UnknownApiExpression),
    }
}
