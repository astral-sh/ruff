use crate::{
    Db, ProgramEnvironment,
    types::{
        BindingContext, BoundTypeVarIdentity, BoundTypeVarInstance, StaticClassLiteral, Type,
        attribute_write::{DescriptorSetterDomain, descriptor_setter_domain},
    },
};

mod equations;

pub(super) use equations::{VarianceOrigin, VarianceTerm, infer_protocol_variance};

impl<'db> StaticClassLiteral<'db> {
    /// Keeps `Self` symbolic while inspecting a class's interface. Substituting `C[T]` would
    /// incorrectly make a parameter annotated as `Self` consume the class's `T`.
    pub(super) fn variance_receiver(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Type<'db> {
        Type::TypeVar(BoundTypeVarInstance::synthetic_self(
            db,
            Type::instance(db, env, self.identity_specialization(db)),
            BindingContext::Definition(self.definition(db)),
        ))
    }
}

/// The read and write contributions of one exposed member, before attribute mutability or
/// source-specific exclusions are applied.
#[derive(Clone, Copy)]
pub(super) struct MemberVariance<'db> {
    pub(super) read_ty: Type<'db>,
    pub(super) write_domain: DescriptorSetterDomain<'db>,
}

impl<'db> MemberVariance<'db> {
    /// Resolves the instance read and descriptor write types of a class member.
    pub(super) fn of(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        receiver: Type<'db>,
    ) -> Self {
        if let Type::SlotDescriptor(descriptor) = ty {
            // The built-in descriptor's untyped `__set__` loses the slot's stored value type.
            let value_ty = descriptor.value_type(db);
            return Self {
                read_ty: value_ty,
                write_domain: DescriptorSetterDomain::Known(value_ty),
            };
        }
        Self {
            read_ty: Self::bind(db, env, ty, receiver),
            write_domain: descriptor_setter_domain(db, env, ty, receiver),
        }
    }

    /// An accessor contributes its bound callable signature, including a setter's input.
    pub(super) fn accessor(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        receiver: Type<'db>,
    ) -> Self {
        Self {
            read_ty: Self::bind(db, env, ty, receiver),
            write_domain: DescriptorSetterDomain::Missing,
        }
    }

    fn bind(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        receiver: Type<'db>,
    ) -> Type<'db> {
        ty.try_call_dunder_get(db, env, Some(receiver), receiver.to_meta_type(db, env))
            .unwrap_or_else(|error| Some(error.fallback()))
            .map_or(ty, |result| result.return_type)
    }
}

impl<'db> VarianceInferable<'db> for MemberVariance<'db> {
    fn variance_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        let write = match self.write_domain {
            DescriptorSetterDomain::Known(ty) => ty
                .with_polarity(TypeVarVariance::Contravariant)
                .variance_of(db, env, typevar),
            // An unresolved write domain does not erase a known read requirement.
            DescriptorSetterDomain::Missing | DescriptorSetterDomain::Deferred => {
                VarianceTerm::BIVARIANT
            }
        };
        VarianceTerm::join(db, [self.read_ty.variance_of(db, env, typevar), write])
    }
}

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

pub(crate) trait VarianceInferable<'db>: Sized {
    /// Builds a variance expression without choosing how protocol declarations are evaluated.
    ///
    /// Recursive definitions contribute named variables instead of expanding their bodies.
    /// Evaluation and dependency discovery operate on the resulting expression, so both use
    /// the same member-selection and variance-composition rules.
    fn variance_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db>;

    /// Creates a `VarianceInferable` that applies `polarity` (see
    /// `TypeVarVariance::compose`) to the result of variance inference on the
    /// underlying value.
    ///
    /// In some cases, we need to apply a polarity to the recursive call.
    /// You can do this with `ty.with_polarity(polarity).variance_of(db, env, typevar)`.
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
    fn variance_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        let WithPolarity {
            variance_inferable,
            polarity,
        } = self;

        VarianceTerm::from(polarity)
            .compose_thunk(db, || variance_inferable.variance_of(db, env, typevar))
    }
}
