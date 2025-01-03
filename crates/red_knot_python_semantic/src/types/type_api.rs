use crate::declare_lint;
use crate::lint::{Level, LintRegistryBuilder, LintStatus};
use crate::types::{Class, IntersectionBuilder, Type};
use crate::Db;

#[derive(Debug)]
pub(crate) enum TypeApiError<'db> {
    /// Wrong number of arguments in a type API call
    WrongNumberOfArguments { expected: usize, actual: usize },
    /// Argument of `assert_true` did not have type `Literal[True]`
    StaticAssertionError(Type<'db>),
    /// Unknown type API expression
    UnknownAttribute,
}

type Result<'db, T> = std::result::Result<T, TypeApiError<'db>>;

fn expect_n_arguments<'db, const N: usize>(
    mut arguments: impl Iterator<Item = Type<'db>>,
) -> Result<'db, [Type<'db>; N]> {
    let mut result = [Type::Unknown; N];
    for i in 0..N {
        result[i] = arguments
            .next()
            .ok_or(TypeApiError::WrongNumberOfArguments {
                expected: N,
                actual: i,
            })?;
    }
    if arguments.next().is_some() {
        let actual = N + 1 + arguments.count();
        return Err(TypeApiError::WrongNumberOfArguments {
            expected: N,
            actual,
        });
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
        _ => Err(TypeApiError::UnknownAttribute),
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
                Err(TypeApiError::StaticAssertionError(ty))
            }
        }

        _ => Err(TypeApiError::UnknownAttribute),
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
    /// assert_true(1 + 1 == 3)  # error: evaluates to `False`
    ///
    /// assert_true(int(2.0 * 3.0) == 6)  # error: does not have a statically known truthiness
    /// ```
    pub(crate) static TYPE_API_STATIC_ASSERTION_ERROR = {
        summary: "Failed static assertion",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

pub(crate) fn register_lints(registry: &mut LintRegistryBuilder) {
    registry.register_lint(&TYPE_API_WRONG_ARITY);
    registry.register_lint(&TYPE_API_STATIC_ASSERTION_ERROR);
}
