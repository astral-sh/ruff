use crate::{Db, types::BoundTypeVarInstance};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub enum TypeVarVariance {
    Invariant,
    Covariant,
    Contravariant,
    Bivariant,
}

impl TypeVarVariance {
    pub const fn bottom() -> Self {
        TypeVarVariance::Bivariant
    }

    pub const fn top() -> Self {
        TypeVarVariance::Invariant
    }

    // supremum
    #[must_use]
    pub(crate) const fn join(self, other: Self) -> Self {
        use TypeVarVariance::{Bivariant, Contravariant, Covariant, Invariant};
        match (self, other) {
            (Invariant, _) | (_, Invariant) => Invariant,
            (Covariant, Covariant) => Covariant,
            (Contravariant, Contravariant) => Contravariant,
            (Covariant, Contravariant) | (Contravariant, Covariant) => Invariant,
            (Bivariant, other) | (other, Bivariant) => other,
        }
    }

    /// Compose two variances: useful for combining use-site and definition-site variances, e.g.
    /// `C[D[T]]` or function argument/return position variances.
    ///
    /// `other` is a thunk to avoid unnecessary computation when `self` is `Bivariant`.
    ///
    /// Based on the variance composition/transformation operator in
    /// <https://people.cs.umass.edu/~yannis/variance-extended2011.pdf>, page 5
    ///
    /// While their operation would have `compose(Invariant, Bivariant) ==
    /// Invariant`, we instead have it evaluate to `Bivariant`. This is a valid
    /// choice, as discussed on that same page, where type equality is semantic
    /// rather than syntactic. To see that this holds for our setting consider
    /// the type
    /// ```python
    /// type ConstantInt[T] = int
    /// ```
    /// We would say `ConstantInt[str]` = `ConstantInt[float]`, so we qualify as
    /// using semantic equivalence.
    #[must_use]
    pub(crate) fn compose(self, other: Self) -> Self {
        self.compose_thunk(|| other)
    }

    /// Like `compose`, but takes `other` as a thunk to avoid unnecessary
    /// computation when `self` is `Bivariant`.
    #[must_use]
    pub(crate) fn compose_thunk<F>(self, other: F) -> Self
    where
        F: FnOnce() -> Self,
    {
        match self {
            TypeVarVariance::Covariant => other(),
            TypeVarVariance::Contravariant => other().flip(),
            TypeVarVariance::Bivariant => TypeVarVariance::Bivariant,
            TypeVarVariance::Invariant => {
                if TypeVarVariance::Bivariant == other() {
                    TypeVarVariance::Bivariant
                } else {
                    TypeVarVariance::Invariant
                }
            }
        }
    }

    /// Flips the polarity of the variance.
    ///
    /// Covariant becomes contravariant, contravariant becomes covariant, others remain unchanged.
    pub(crate) const fn flip(self) -> Self {
        match self {
            TypeVarVariance::Invariant => TypeVarVariance::Invariant,
            TypeVarVariance::Covariant => TypeVarVariance::Contravariant,
            TypeVarVariance::Contravariant => TypeVarVariance::Covariant,
            TypeVarVariance::Bivariant => TypeVarVariance::Bivariant,
        }
    }

    pub(crate) const fn is_covariant(self) -> bool {
        matches!(
            self,
            TypeVarVariance::Covariant | TypeVarVariance::Bivariant
        )
    }
}

impl std::iter::FromIterator<Self> for TypeVarVariance {
    fn from_iter<T: IntoIterator<Item = Self>>(iter: T) -> Self {
        use std::ops::ControlFlow;
        // TODO: use `into_value` when control_flow_into_value is stable
        let (ControlFlow::Break(variance) | ControlFlow::Continue(variance)) = iter
            .into_iter()
            .try_fold(TypeVarVariance::Bivariant, |acc, variance| {
                let supremum = acc.join(variance);
                match supremum {
                    // short circuit at top
                    TypeVarVariance::Invariant => ControlFlow::Break(supremum),
                    TypeVarVariance::Bivariant
                    | TypeVarVariance::Covariant
                    | TypeVarVariance::Contravariant => ControlFlow::Continue(supremum),
                }
            });
        variance
    }
}

pub(crate) trait VarianceInferable<'db>: Sized {
    /// The variance of `typevar` in `self`
    ///
    /// Generally, one will implement this by traversing any types within `self`
    /// in which `typevar` could occur, and calling `variance_of` recursively on
    /// them.
    ///
    /// Sometimes the recursive calls will be in positions where you need to
    /// specify a non-covariant polarity. See `with_polarity` for more details.
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance;

    /// Creates a `VarianceInferable` that applies `polarity` (see
    /// `TypeVarVariance::compose`) to the result of variance inference on the
    /// underlying value.
    ///
    /// In some cases, we need to apply a polarity to the recursive call.
    /// You can do this with `ty.with_polarity(polarity).variance_of(typevar)`.
    /// Generally, this will be whenever the type occurs in argument-position,
    /// in which case you will want `TypeVarVariance::Contravariant`, or
    /// `TypeVarVariance::Invariant` if the value(s) being annotated is known to
    /// be mutable, such as `T` in `list[T]`. See the [typing spec][typing-spec]
    /// for more details.
    ///
    /// [typing-spec]: https://typing.python.org/en/latest/spec/generics.html#variance
    fn with_polarity(self, polarity: TypeVarVariance) -> impl VarianceInferable<'db> {
        WithPolarity {
            variance_inferable: self,
            polarity,
        }
    }
}

pub(crate) struct WithPolarity<T> {
    variance_inferable: T,
    polarity: TypeVarVariance,
}

impl<'db, T> VarianceInferable<'db> for WithPolarity<T>
where
    T: VarianceInferable<'db>,
{
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        let WithPolarity {
            variance_inferable,
            polarity,
        } = self;

        polarity.compose_thunk(|| variance_inferable.variance_of(db, typevar))
    }
}
