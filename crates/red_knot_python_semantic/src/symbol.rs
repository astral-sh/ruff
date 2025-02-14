use crate::{
    types::{todo_type, Type, UnionType},
    Db,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Boundness {
    Bound,
    PossiblyUnbound,
}

/// The result of a symbol lookup, which can either be a (possibly unbound) type
/// or a completely unbound symbol.
///
/// Consider this example:
/// ```py
/// bound = 1
///
/// if flag:
///     possibly_unbound = 2
/// ```
///
/// If we look up symbols in this scope, we would get the following results:
/// ```rs
/// bound:             Symbol::Type(Type::IntLiteral(1), Boundness::Bound),
/// possibly_unbound:  Symbol::Type(Type::IntLiteral(2), Boundness::PossiblyUnbound),
/// non_existent:      Symbol::Unbound,
/// ```
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub(crate) enum Symbol<'db> {
    Type(Type<'db>, Boundness),
    Unbound,
}

impl<'db> Symbol<'db> {
    /// Constructor that creates a `Symbol` with boundness [`Boundness::Bound`].
    pub(crate) fn bound(ty: impl Into<Type<'db>>) -> Self {
        Symbol::Type(ty.into(), Boundness::Bound)
    }

    /// Constructor that creates a [`Symbol`] with a [`crate::types::TodoType`] type
    /// and boundness [`Boundness::Bound`].
    #[allow(unused_variables)] // Only unused in release builds
    pub(crate) fn todo(message: &'static str) -> Self {
        Symbol::Type(todo_type!(message), Boundness::Bound)
    }

    pub(crate) fn is_unbound(&self) -> bool {
        matches!(self, Symbol::Unbound)
    }

    /// Returns the type of the symbol, ignoring possible unboundness.
    ///
    /// If the symbol is *definitely* unbound, this function will return `None`. Otherwise,
    /// if there is at least one control-flow path where the symbol is bound, return the type.
    pub(crate) fn ignore_possibly_unbound(&self) -> Option<Type<'db>> {
        match self {
            Symbol::Type(ty, _) => Some(*ty),
            Symbol::Unbound => None,
        }
    }

    #[cfg(test)]
    #[track_caller]
    pub(crate) fn expect_type(self) -> Type<'db> {
        self.ignore_possibly_unbound()
            .expect("Expected a (possibly unbound) type, not an unbound symbol")
    }

    /// Transform the symbol into a [`LookupResult`],
    /// a [`Result`] type in which the `Ok` variant represents a definitely bound symbol
    /// and the `Err` variant represents a symbol that is either definitely or possibly unbound.
    pub(crate) fn into_lookup_result(self) -> LookupResult<'db> {
        match self {
            Symbol::Type(ty, Boundness::Bound) => Ok(ty),
            Symbol::Type(ty, Boundness::PossiblyUnbound) => Err(LookupError::PossiblyUnbound(ty)),
            Symbol::Unbound => Err(LookupError::Unbound),
        }
    }

    /// Safely unwrap the symbol into a [`Type`].
    ///
    /// If the symbol is definitely unbound or possibly unbound, it will be transformed into a
    /// [`LookupError`] and `diagnostic_fn` will be applied to the error value before returning
    /// the result of `diagnostic_fn` (which will be a [`Type`]). This allows the caller to ensure
    /// that a diagnostic is emitted if the symbol is possibly or definitely unbound.
    pub(crate) fn unwrap_with_diagnostic(
        self,
        diagnostic_fn: impl FnOnce(LookupError<'db>) -> Type<'db>,
    ) -> Type<'db> {
        self.into_lookup_result().unwrap_or_else(diagnostic_fn)
    }

    /// Fallback (partially or fully) to another symbol if `self` is partially or fully unbound.
    ///
    /// 1. If `self` is definitely bound, return `self` without evaluating `fallback_fn()`.
    /// 2. Else, evaluate `fallback_fn()`:
    ///    a. If `self` is definitely unbound, return the result of `fallback_fn()`.
    ///    b. Else, if `fallback` is definitely unbound, return `self`.
    ///    c. Else, if `self` is possibly unbound and `fallback` is definitely bound,
    ///       return `Symbol(<union of self-type and fallback-type>, Boundness::Bound)`
    ///    d. Else, if `self` is possibly unbound and `fallback` is possibly unbound,
    ///       return `Symbol(<union of self-type and fallback-type>, Boundness::PossiblyUnbound)`
    #[must_use]
    pub(crate) fn or_fall_back_to(
        self,
        db: &'db dyn Db,
        fallback_fn: impl FnOnce() -> Self,
    ) -> Self {
        self.into_lookup_result()
            .or_else(|lookup_error| lookup_error.or_fall_back_to(db, fallback_fn()))
            .into()
    }

    #[must_use]
    pub(crate) fn map_type(self, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Symbol<'db> {
        match self {
            Symbol::Type(ty, boundness) => Symbol::Type(f(ty), boundness),
            Symbol::Unbound => Symbol::Unbound,
        }
    }
}

impl<'db> From<LookupResult<'db>> for Symbol<'db> {
    fn from(value: LookupResult<'db>) -> Self {
        match value {
            Ok(ty) => Symbol::Type(ty, Boundness::Bound),
            Err(LookupError::Unbound) => Symbol::Unbound,
            Err(LookupError::PossiblyUnbound(ty)) => Symbol::Type(ty, Boundness::PossiblyUnbound),
        }
    }
}

/// Possible ways in which a symbol lookup can (possibly or definitely) fail.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum LookupError<'db> {
    Unbound,
    PossiblyUnbound(Type<'db>),
}

impl<'db> LookupError<'db> {
    /// Fallback (wholly or partially) to `fallback` to create a new [`LookupResult`].
    pub(crate) fn or_fall_back_to(
        self,
        db: &'db dyn Db,
        fallback: Symbol<'db>,
    ) -> LookupResult<'db> {
        let fallback = fallback.into_lookup_result();
        match (&self, &fallback) {
            (LookupError::Unbound, _) => fallback,
            (LookupError::PossiblyUnbound { .. }, Err(LookupError::Unbound)) => Err(self),
            (LookupError::PossiblyUnbound(ty), Ok(ty2)) => {
                Ok(UnionType::from_elements(db, [ty, ty2]))
            }
            (LookupError::PossiblyUnbound(ty), Err(LookupError::PossiblyUnbound(ty2))) => Err(
                LookupError::PossiblyUnbound(UnionType::from_elements(db, [ty, ty2])),
            ),
        }
    }
}

/// A [`Result`] type in which the `Ok` variant represents a definitely bound symbol
/// and the `Err` variant represents a symbol that is either definitely or possibly unbound.
pub(crate) type LookupResult<'db> = Result<Type<'db>, LookupError<'db>>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::setup_db;

    #[test]
    fn test_symbol_or_fall_back_to() {
        use Boundness::{Bound, PossiblyUnbound};

        let db = setup_db();
        let ty1 = Type::IntLiteral(1);
        let ty2 = Type::IntLiteral(2);

        // Start from an unbound symbol
        assert_eq!(
            Symbol::Unbound.or_fall_back_to(&db, || Symbol::Unbound),
            Symbol::Unbound
        );
        assert_eq!(
            Symbol::Unbound.or_fall_back_to(&db, || Symbol::Type(ty1, PossiblyUnbound)),
            Symbol::Type(ty1, PossiblyUnbound)
        );
        assert_eq!(
            Symbol::Unbound.or_fall_back_to(&db, || Symbol::Type(ty1, Bound)),
            Symbol::Type(ty1, Bound)
        );

        // Start from a possibly unbound symbol
        assert_eq!(
            Symbol::Type(ty1, PossiblyUnbound).or_fall_back_to(&db, || Symbol::Unbound),
            Symbol::Type(ty1, PossiblyUnbound)
        );
        assert_eq!(
            Symbol::Type(ty1, PossiblyUnbound)
                .or_fall_back_to(&db, || Symbol::Type(ty2, PossiblyUnbound)),
            Symbol::Type(UnionType::from_elements(&db, [ty1, ty2]), PossiblyUnbound)
        );
        assert_eq!(
            Symbol::Type(ty1, PossiblyUnbound).or_fall_back_to(&db, || Symbol::Type(ty2, Bound)),
            Symbol::Type(UnionType::from_elements(&db, [ty1, ty2]), Bound)
        );

        // Start from a definitely bound symbol
        assert_eq!(
            Symbol::Type(ty1, Bound).or_fall_back_to(&db, || Symbol::Unbound),
            Symbol::Type(ty1, Bound)
        );
        assert_eq!(
            Symbol::Type(ty1, Bound).or_fall_back_to(&db, || Symbol::Type(ty2, PossiblyUnbound)),
            Symbol::Type(ty1, Bound)
        );
        assert_eq!(
            Symbol::Type(ty1, Bound).or_fall_back_to(&db, || Symbol::Type(ty2, Bound)),
            Symbol::Type(ty1, Bound)
        );
    }
}
