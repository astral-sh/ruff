use crate::{
    types::{Type, UnionType},
    Db,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Boundness {
    Bound,
    MayBeUnbound,
}

/// The result of a symbol lookup, which can either be a (possibly unbound) type
/// or a completely unbound symbol.
///
/// Consider this example:
/// ```py
/// bound = 1
///
/// if flag:
///     maybe_unbound = 2
/// ```
///
/// If we look up symbols in this scope, we would get the following results:
/// ```rs
/// bound:          Symbol::Type(Type::IntLiteral(1), Boundness::Bound),
/// maybe_unbound:  Symbol::Type(Type::IntLiteral(2), Boundness::MayBeUnbound),
/// non_existent:   Symbol::Unbound,
/// ```
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Symbol<'db> {
    Type(Type<'db>, Boundness),
    Unbound,
}

impl<'db> Symbol<'db> {
    pub(crate) fn is_unbound(&self) -> bool {
        matches!(self, Symbol::Unbound)
    }

    pub(crate) fn may_be_unbound(&self) -> bool {
        match self {
            Symbol::Type(_, Boundness::MayBeUnbound) | Symbol::Unbound => true,
            Symbol::Type(_, Boundness::Bound) => false,
        }
    }

    pub(crate) fn unwrap_or_unknown(&self) -> Type<'db> {
        match self {
            Symbol::Type(ty, _) => *ty,
            Symbol::Unbound => Type::Unknown,
        }
    }

    pub(crate) fn as_type(&self) -> Option<Type<'db>> {
        match self {
            Symbol::Type(ty, _) => Some(*ty),
            Symbol::Unbound => None,
        }
    }

    #[cfg(test)]
    #[track_caller]
    pub(crate) fn expect_type(self) -> Type<'db> {
        self.as_type()
            .expect("Expected a (possibly unbound) type, not an unbound symbol")
    }

    #[must_use]
    pub(crate) fn replace_unbound_with(
        self,
        db: &'db dyn Db,
        replacement: &Symbol<'db>,
    ) -> Symbol<'db> {
        match replacement {
            Symbol::Type(replacement, _) => Symbol::Type(
                match self {
                    Symbol::Type(ty, Boundness::Bound) => ty,
                    Symbol::Type(ty, Boundness::MayBeUnbound) => {
                        UnionType::from_elements(db, [*replacement, ty])
                    }
                    Symbol::Unbound => *replacement,
                },
                Boundness::Bound,
            ),
            Symbol::Unbound => self,
        }
    }
}
