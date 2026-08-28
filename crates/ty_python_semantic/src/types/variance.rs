use crate::Db;
use crate::{
    ProgramEnvironment,
    types::{BoundTypeVarIdentity, StaticClassLiteral},
};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, get_size2::GetSize)]
pub enum TypeVarVariance {
    Invariant,
    Covariant,
    Contravariant,
    Bivariant,
}

impl TypeVarVariance {
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

    pub(crate) const fn is_contravariant(self) -> bool {
        matches!(
            self,
            TypeVarVariance::Contravariant | TypeVarVariance::Bivariant
        )
    }

    /// Returns a human-readable name for this variance, matching the keyword
    /// argument names used in `TypeVar(covariant=True)` / `TypeVar(contravariant=True)`.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            TypeVarVariance::Invariant => "invariant",
            TypeVarVariance::Covariant => "covariant",
            TypeVarVariance::Contravariant => "contravariant",
            TypeVarVariance::Bivariant => "bivariant",
        }
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

/// Controls whether protocol specializations use declared variance or their structural fixed point.
///
/// Ordinary type relationships must honor a protocol parameter's effective variance. Validating
/// the declaration itself instead has to follow recursive protocol references structurally;
/// otherwise the declaration being checked would determine its own inferred result.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum VarianceInferenceMode<'db> {
    /// Honor explicit variance declarations, inferring variance only when no declaration exists.
    Effective,
    /// Infer protocol parameters that depend on this parameter from their interfaces.
    /// Independent parameters retain their declared variance.
    Structural(BoundTypeVarIdentity<'db>),
    /// Honor declarations while tracking variance dependencies on this parameter.
    Dependencies(BoundTypeVarIdentity<'db>),
}

impl<'db> VarianceInferenceMode<'db> {
    /// The variance of a supported protocol parameter at a use site. Returning `None` asks the
    /// caller to infer its interface instead of using the declaration.
    ///
    /// A path back to the root puts both parameters in the same recursive component. The class
    /// query uses formal parameters, not specializations, so expanding recursive references such
    /// as `P[list[T]]` cannot produce an unbounded sequence of dependency queries.
    pub(super) fn protocol_parameter_variance(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        typevar: BoundTypeVarIdentity<'db>,
        declared: TypeVarVariance,
    ) -> Option<VarianceResult> {
        let depends_on_root = |root| {
            typevar == root
                || class
                    .variance_of_in_mode(db, env, typevar, Self::Dependencies(root))
                    .depends_on_root
        };
        match self {
            Self::Structural(root) if depends_on_root(root) => None,
            Self::Dependencies(root) => Some(VarianceResult {
                variance: declared,
                depends_on_root: depends_on_root(root),
            }),
            Self::Effective | Self::Structural(_) => Some(declared.into()),
        }
    }

    /// Join occurrences, retaining dependencies even after variance has reached `Invariant`.
    /// Ordinary inference can stop at `Invariant`; dependency analysis also needs to find any
    /// reference to the root in subsequent members or specialization arguments.
    pub(super) fn join(
        self,
        occurrences: impl IntoIterator<Item = VarianceResult>,
    ) -> VarianceResult {
        let mut result = VarianceResult::BIVARIANT;
        for occurrence in occurrences {
            result.variance = result.variance.join(occurrence.variance);
            result.depends_on_root |= occurrence.depends_on_root;
            if result.variance == TypeVarVariance::Invariant
                && (!matches!(self, Self::Dependencies(_)) || result.depends_on_root)
            {
                break;
            }
        }
        result
    }
}

/// Variance and, during dependency analysis, whether it refers to the parameter being validated.
///
/// Both are computed by the same traversal. Composition removes dependencies along with unused
/// type arguments: `type Ignore[T] = int` makes `Ignore[P[T]]` independent of both `T` and `P`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, get_size2::GetSize)]
pub(crate) struct VarianceResult {
    pub(super) variance: TypeVarVariance,
    pub(super) depends_on_root: bool,
}

impl VarianceResult {
    pub(super) const BIVARIANT: Self = Self {
        variance: TypeVarVariance::Bivariant,
        depends_on_root: false,
    };

    pub(super) fn compose_thunk(self, other: impl FnOnce() -> Self) -> Self {
        if self.variance == TypeVarVariance::Bivariant {
            return Self::BIVARIANT;
        }
        let other = other();
        let variance = self.variance.compose(other.variance);
        Self {
            variance,
            depends_on_root: variance != TypeVarVariance::Bivariant
                && (self.depends_on_root || other.depends_on_root),
        }
    }
}

impl From<TypeVarVariance> for VarianceResult {
    fn from(variance: TypeVarVariance) -> Self {
        Self {
            variance,
            depends_on_root: false,
        }
    }
}

pub(crate) trait VarianceInferable<'db>: Sized {
    /// The variance of `typevar` in `self`, honoring explicit variance declarations.
    fn variance_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> TypeVarVariance {
        self.variance_of_in_mode(db, env, typevar, VarianceInferenceMode::Effective)
            .variance
    }

    /// Computes variance while preserving the inference mode through nested types and Salsa keys.
    ///
    /// Implementations traverse types within `self` in which `typevar` could occur, calling this
    /// method recursively with the same mode. Use `with_polarity` for non-covariant positions,
    /// and `mode.join` to combine occurrences without dropping dependency information.
    fn variance_of_in_mode(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
        mode: VarianceInferenceMode<'db>,
    ) -> VarianceResult;

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
    fn variance_of_in_mode(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
        mode: VarianceInferenceMode<'db>,
    ) -> VarianceResult {
        let WithPolarity {
            variance_inferable,
            polarity,
        } = self;

        VarianceResult::from(polarity)
            .compose_thunk(|| variance_inferable.variance_of_in_mode(db, env, typevar, mode))
    }
}

#[cfg(test)]
mod tests {
    use super::{TypeVarVariance, VarianceResult};

    #[test]
    fn composition_erases_dependencies_in_either_position() {
        for variance in [
            TypeVarVariance::Covariant,
            TypeVarVariance::Contravariant,
            TypeVarVariance::Invariant,
        ] {
            let dependent = VarianceResult {
                variance,
                depends_on_root: true,
            };
            assert_eq!(
                dependent.compose_thunk(|| VarianceResult::BIVARIANT),
                VarianceResult::BIVARIANT,
            );
            assert_eq!(
                VarianceResult::BIVARIANT.compose_thunk(|| dependent),
                VarianceResult::BIVARIANT,
            );
        }
    }
}
