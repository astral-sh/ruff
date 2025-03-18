use std::cell::Cell;
use std::rc::Rc;

use super::Type;

/// Typed arguments for a single call, in source order, with an optional bound self/cls parameter.
#[derive(Clone, Debug, Default)]
pub(crate) struct CallArguments<'a, 'db> {
    bound_self: Option<Argument<'a, 'db>>,
    arguments: Rc<[Argument<'a, 'db>]>,
}

impl<'a, 'db> CallArguments<'a, 'db> {
    /// Create a [`CallArguments`] with no arguments.
    pub(crate) fn none() -> Self {
        Self {
            bound_self: None,
            arguments: vec![].into(),
        }
    }

    /// Create a [`CallArguments`] from an iterator over non-variadic positional argument types.
    pub(crate) fn positional(positional_tys: impl IntoIterator<Item = Type<'db>>) -> Self {
        positional_tys
            .into_iter()
            .map(|ty| Argument::positional().with_argument_type(ty))
            .collect()
    }

    /// Prepend an extra positional argument.
    pub(crate) fn with_self(&self, self_ty: Type<'db>) -> Self {
        assert!(
            self.bound_self.is_none(),
            "cannot bind multiple self/cls parameters"
        );
        Self {
            bound_self: Some(Argument::synthetic().with_argument_type(self_ty)),
            arguments: self.arguments.clone(),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.arguments.len() + usize::from(self.bound_self.is_some())
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Argument<'a, 'db>> {
        self.bound_self.iter().chain(self.arguments.as_ref())
    }

    // TODO this should be eliminated in favor of [`bind_call`]
    pub(crate) fn first_argument(&self) -> Option<Type<'db>> {
        self.iter().next().map(Argument::argument_type)
    }

    // TODO this should be eliminated in favor of [`bind_call`]
    pub(crate) fn second_argument(&self) -> Option<Type<'db>> {
        self.iter().nth(1).map(Argument::argument_type)
    }

    // TODO this should be eliminated in favor of [`bind_call`]
    pub(crate) fn third_argument(&self) -> Option<Type<'db>> {
        self.iter().nth(2).map(Argument::argument_type)
    }
}

impl<'a, 'db> FromIterator<Argument<'a, 'db>> for CallArguments<'a, 'db> {
    fn from_iter<T: IntoIterator<Item = Argument<'a, 'db>>>(iter: T) -> Self {
        Self {
            bound_self: None,
            arguments: iter.into_iter().collect(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Argument<'a, 'db> {
    kind: ArgumentKind<'a>,
    /// The inferred type of this argument.
    argument_type: Cell<Type<'db>>,
}

impl<'a, 'db> Argument<'a, 'db> {
    pub(crate) fn keyword(name: &'a str) -> Self {
        Self {
            kind: ArgumentKind::Keyword(name),
            argument_type: Cell::new(Type::unknown()),
        }
    }

    pub(crate) fn keywords() -> Self {
        Self {
            kind: ArgumentKind::Keywords,
            argument_type: Cell::new(Type::unknown()),
        }
    }

    pub(crate) fn positional() -> Self {
        Self {
            kind: ArgumentKind::Positional,
            argument_type: Cell::new(Type::unknown()),
        }
    }

    pub(crate) fn synthetic() -> Self {
        Self {
            kind: ArgumentKind::Synthetic,
            argument_type: Cell::new(Type::unknown()),
        }
    }

    pub(crate) fn variadic() -> Self {
        Self {
            kind: ArgumentKind::Variadic,
            argument_type: Cell::new(Type::unknown()),
        }
    }

    pub(crate) fn with_argument_type(self, argument_type: Type<'db>) -> Self {
        self.argument_type.set(argument_type);
        self
    }

    pub(crate) fn set_argument_type(&self, argument_type: Type<'db>) {
        self.argument_type.set(argument_type);
    }

    pub(crate) fn kind(&self) -> ArgumentKind<'a> {
        self.kind
    }

    pub(crate) fn argument_type(&self) -> Type<'db> {
        self.argument_type.get()
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ArgumentKind<'a> {
    /// The synthetic `self` or `cls` argument, which doesn't appear explicitly at the call site.
    Synthetic,
    /// A positional argument.
    Positional,
    /// A starred positional argument (e.g. `*args`).
    Variadic,
    /// A keyword argument (e.g. `a=1`).
    Keyword(&'a str),
    /// The double-starred keywords argument (e.g. `**kwargs`).
    Keywords,
}
