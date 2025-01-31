use crate::{
    types::{Type, UnionType},
    Db,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Boundness {
    Bound,
    PossiblyUnbound,
}

impl Boundness {
    pub(crate) fn or(self, other: Boundness) -> Boundness {
        match (self, other) {
            (Boundness::Bound, _) | (_, Boundness::Bound) => Boundness::Bound,
            (Boundness::PossiblyUnbound, Boundness::PossiblyUnbound) => Boundness::PossiblyUnbound,
        }
    }
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Symbol<'db> {
    Type(Type<'db>, Boundness),
    Unbound,
}

impl<'db> Symbol<'db> {
    pub(crate) fn is_unbound(&self) -> bool {
        matches!(self, Symbol::Unbound)
    }

    pub(crate) fn possibly_unbound(&self) -> bool {
        match self {
            Symbol::Type(_, Boundness::PossiblyUnbound) | Symbol::Unbound => true,
            Symbol::Type(_, Boundness::Bound) => false,
        }
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

    #[must_use]
    pub(crate) fn or_fall_back_to(self, db: &'db dyn Db, fallback: &Symbol<'db>) -> Symbol<'db> {
        match fallback {
            Symbol::Type(fallback_ty, fallback_boundness) => match self {
                Symbol::Type(_, Boundness::Bound) => self,
                Symbol::Type(ty, boundness @ Boundness::PossiblyUnbound) => Symbol::Type(
                    UnionType::from_elements(db, [*fallback_ty, ty]),
                    fallback_boundness.or(boundness),
                ),
                Symbol::Unbound => fallback.clone(),
            },
            Symbol::Unbound => self,
        }
    }

    #[must_use]
    pub(crate) fn map_type(self, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Symbol<'db> {
        match self {
            Symbol::Type(ty, boundness) => Symbol::Type(f(ty), boundness),
            Symbol::Unbound => Symbol::Unbound,
        }
    }
}

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
            Symbol::Unbound.or_fall_back_to(&db, &Symbol::Unbound),
            Symbol::Unbound
        );
        assert_eq!(
            Symbol::Unbound.or_fall_back_to(&db, &Symbol::Type(ty1, PossiblyUnbound)),
            Symbol::Type(ty1, PossiblyUnbound)
        );
        assert_eq!(
            Symbol::Unbound.or_fall_back_to(&db, &Symbol::Type(ty1, Bound)),
            Symbol::Type(ty1, Bound)
        );

        // Start from a possibly unbound symbol
        assert_eq!(
            Symbol::Type(ty1, PossiblyUnbound).or_fall_back_to(&db, &Symbol::Unbound),
            Symbol::Type(ty1, PossiblyUnbound)
        );
        assert_eq!(
            Symbol::Type(ty1, PossiblyUnbound)
                .or_fall_back_to(&db, &Symbol::Type(ty2, PossiblyUnbound)),
            Symbol::Type(UnionType::from_elements(&db, [ty2, ty1]), PossiblyUnbound)
        );
        assert_eq!(
            Symbol::Type(ty1, PossiblyUnbound).or_fall_back_to(&db, &Symbol::Type(ty2, Bound)),
            Symbol::Type(UnionType::from_elements(&db, [ty2, ty1]), Bound)
        );

        // Start from a definitely bound symbol
        assert_eq!(
            Symbol::Type(ty1, Bound).or_fall_back_to(&db, &Symbol::Unbound),
            Symbol::Type(ty1, Bound)
        );
        assert_eq!(
            Symbol::Type(ty1, Bound).or_fall_back_to(&db, &Symbol::Type(ty2, PossiblyUnbound)),
            Symbol::Type(ty1, Bound)
        );
        assert_eq!(
            Symbol::Type(ty1, Bound).or_fall_back_to(&db, &Symbol::Type(ty2, Bound)),
            Symbol::Type(ty1, Bound)
        );
    }
}
