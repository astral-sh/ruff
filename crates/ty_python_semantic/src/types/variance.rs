use crate::{Db, types::TypeVarInstance};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update)]
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
}

impl TypeVarVariance {
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
}

pub(crate) trait VarianceInferable<'db>: Sized {
    fn variance_of(self, db: &'db dyn Db, type_var: TypeVarInstance<'db>) -> TypeVarVariance;

    fn with_polarity(self, polarity: TypeVarVariance) -> WithPolarity<Self> {
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
    // Based on the variance composition/transformation operator in
    // https://people.cs.umass.edu/~yannis/variance-extended2011.pdf, page 5
    //
    // While their operation has compose(invariant, bivariant) = invariant, we
    // instead have it evaluate to bivariant. This is a valid choice, as
    // discussed on that same page, where type equality is semantic rather than
    // syntactic. To see that this holds for our setting consider the type
    // ```python
    // type ConstantInt[T] = int
    // ```
    // We would say `ConstantInt[str]` = `ConstantInt[float]`, so we qualify as
    // using semantic equivalence.
    fn variance_of(self, db: &'db dyn Db, type_var: TypeVarInstance<'db>) -> TypeVarVariance {
        let WithPolarity {
            variance_inferable,
            polarity,
        } = self;
        match polarity {
            TypeVarVariance::Covariant => variance_inferable.variance_of(db, type_var),
            TypeVarVariance::Contravariant => variance_inferable.variance_of(db, type_var).flip(),
            TypeVarVariance::Bivariant => TypeVarVariance::Bivariant,
            TypeVarVariance::Invariant => {
                if TypeVarVariance::Bivariant == variance_inferable.variance_of(db, type_var) {
                    TypeVarVariance::Bivariant
                } else {
                    TypeVarVariance::Invariant
                }
            }
        }
    }
}
