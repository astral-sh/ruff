use crate::declare_lint;
use crate::lint::{Level, LintRegistryBuilder, LintStatus};
use crate::types::{Class, IntersectionBuilder, Type};
use crate::Db;

use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum TypeApiError<'db> {
    #[error("Wrong number of arguments, expected {0}")]
    WrongArity(usize),
    #[error("Failed assertion")]
    FailedAssertion(Type<'db>),
    #[error("Unknown API expression")]
    UnknownApiExpression,
}

type Result<'db, T> = std::result::Result<T, TypeApiError<'db>>;

fn expect_n_arguments<'db, const N: usize>(
    mut arguments: impl Iterator<Item = Type<'db>>,
) -> Result<'db, [Type<'db>; N]> {
    let mut result = [Type::Unknown; N];
    for i in 0..N {
        result[i] = arguments.next().ok_or(TypeApiError::WrongArity(N))?;
    }
    if arguments.next().is_some() {
        return Err(TypeApiError::WrongArity(N));
    }
    Ok(result)
}

fn expect_one_argument<'db>(arguments: impl Iterator<Item = Type<'db>>) -> Result<'db, Type<'db>> {
    expect_n_arguments::<1>(arguments).map(|[ty]| ty)
}

pub(crate) fn resolve_type_operation<'db>(
    db: &'db dyn Db,
    class: Class<'db>,
    arguments: impl Iterator<Item = Type<'db>>,
) -> Result<'db, Type<'db>> {
    match class.name(db).as_str() {
        "Not" => {
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
) -> Result<'db, Type<'db>> {
    match function {
        // Predicates on types
        "is_equivalent_to" => {
            let [ty_a, ty_b] = expect_n_arguments::<2>(arguments)?;
            Ok(Type::BooleanLiteral(ty_a.is_equivalent_to(db, ty_b)))
        }
        "is_subtype_of" => {
            let [ty_a, ty_b] = expect_n_arguments::<2>(arguments)?;
            Ok(Type::BooleanLiteral(ty_a.is_subtype_of(db, ty_b)))
        }
        "is_assignable_to" => {
            let [ty_a, ty_b] = expect_n_arguments::<2>(arguments)?;
            Ok(Type::BooleanLiteral(ty_a.is_assignable_to(db, ty_b)))
        }
        "is_disjoint_from" => {
            let [ty_a, ty_b] = expect_n_arguments::<2>(arguments)?;
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
                Err(TypeApiError::FailedAssertion(ty))
            }
        }

        _ => Err(TypeApiError::UnknownApiExpression),
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for `red_knot` type API calls with the wrong number of arguments.
    ///
    /// ## Examples
    /// ```python
    /// from red_knot import is_equivalent_to
    ///
    /// is_equivalent_to(int, str, bool)  # error: wrong number of arguments
    /// ```
    pub(crate) static TYPE_API_WRONG_ARITY = {
        summary: "wrong number of arguments",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Makes sure that the argument of `assert_true` has a type of `Literal[True]`.
    ///
    /// ## Examples
    /// ```python
    /// from red_knot import assert_true
    ///
    /// assert_true(1 + 2 == 4)  # error: failed assertion
    /// ```
    pub(crate) static TYPE_API_FAILED_ASSERTION = {
        summary: "wrong number of arguments",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

pub(crate) fn register_lints(registry: &mut LintRegistryBuilder) {
    registry.register_lint(&TYPE_API_WRONG_ARITY);
    registry.register_lint(&TYPE_API_FAILED_ASSERTION);
}
