use crate::Db;
use crate::ProgramEnvironment;
use std::collections::VecDeque;
use std::ops::Deref;

use crate::types::class::{DynamicClassLiteral, DynamicEnumLiteral};
use crate::types::class_base::ClassBase;
use crate::types::generics::Specialization;
use crate::types::{ClassLiteral, ClassType, StaticClassLiteral, Type};

use self::iteration::{MroCursor, MroDirection, mro_next_sync};
use self::root::InlineMroRootEffects;

pub(in crate::types) mod base;
pub(in crate::types) mod c3;
pub(in crate::types) mod collection;
pub(in crate::types) mod construction;
pub(in crate::types) mod dynamic;
mod error;
pub(in crate::types) mod field_reads;
pub(in crate::types) mod iteration;
pub(in crate::types) mod root;
pub(in crate::types) mod source;

#[cfg(test)]
pub(in crate::types) mod attempt;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ancestor_computation_probe;
#[cfg(test)]
mod declaration_integration_tests;

/// The inferred method resolution order of a given class.
///
/// An MRO cannot contain non-specialized generic classes. (This is why [`ClassBase`] contains a
/// [`ClassType`], not a [`StaticClassLiteral`].) Any generic classes in a base class list are always
/// specialized — either because the class is explicitly specialized if there is a subscript
/// expression, or because we create the default specialization if there isn't.
///
/// The MRO of a non-specialized generic class can contain generic classes that are specialized
/// with a typevar from the inheriting class. When the inheriting class is specialized, the MRO of
/// the resulting generic alias will substitute those type variables accordingly. For instance, in
/// the following example, the MRO of `D[int]` includes `C[int]`, and the MRO of `D[U]` includes
/// `C[U]` (which is a generic alias, not a non-specialized generic class):
///
/// ```py
/// class C[T]: ...
/// class D[U](C[U]): ...
/// ```
///
/// See [`ClassType::iter_mro`] for more details.
#[derive(PartialEq, Eq, Clone, Debug, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct Mro<'db>(Box<[ClassBase<'db>]>);

impl<'db> Mro<'db> {
    /// Attempt to resolve the MRO of a given class. Because we derive the MRO from the list of
    /// base classes in the class definition, this operation is performed on a [class
    /// literal][StaticClassLiteral], not a [class type][ClassType]. (You can _also_ get the MRO of a
    /// class type, but this is done by first getting the MRO of the underlying class literal, and
    /// specializing each base class as needed if the class type is a generic alias.)
    ///
    /// In the event that a possible list of bases would (or could) lead to a `TypeError` being
    /// raised at runtime due to an unresolvable MRO, we infer the MRO of the class as being `[<the
    /// class in question>, Unknown, object]`. This seems most likely to reduce the possibility of
    /// cascading errors elsewhere. (For a generic class, the first entry in this fallback MRO uses
    /// the default specialization of the class's type variables.)
    ///
    /// (We emit a diagnostic warning about the runtime `TypeError` in
    /// [`super::infer::infer_scope_types`].)
    pub(super) fn of_static_class(
        db: &'db dyn Db,
        class_literal: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Self, StaticMroError<'db>> {
        #[cfg(test)]
        {
            if salsa::attempt_probe::is_incomplete(db) {
                return Ok(Self::incomplete());
            }
            if crate::types::constructor::expansion_probe::mro_effects_enabled() {
                return construction::static_mro_sync(
                    db,
                    class_literal,
                    specialization,
                    &attempt::AttemptMroEffects::new(db),
                )
                .unwrap_or_else(|_| Ok(Self::incomplete()));
            }
        }
        match construction::static_mro_sync(
            db,
            class_literal,
            specialization,
            &construction::InlineStaticMroEffects::new(db),
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    pub(in crate::types) fn static_cycle(
        db: &'db dyn Db,
        class_literal: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Self, Box<StaticMroError<'db>>> {
        #[cfg(test)]
        {
            if salsa::attempt_probe::is_incomplete(db) {
                return Ok(Self::incomplete());
            }
            if crate::types::constructor::expansion_probe::mro_effects_enabled() {
                return match construction::static_mro_cycle_sync(
                    db,
                    class_literal,
                    specialization,
                    &attempt::AttemptMroEffects::new(db),
                ) {
                    Ok(error) => Err(Box::new(error)),
                    Err(_) => Ok(Self::incomplete()),
                };
            }
        }
        match construction::static_mro_cycle_sync(
            db,
            class_literal,
            specialization,
            &construction::InlineStaticMroEffects::new(db),
        ) {
            Ok(error) => Err(Box::new(error)),
            Err(never) => match never {},
        }
    }

    /// Storage for an already-incomplete query; consumers must check the attempt before
    /// inspecting it. Constructing it neither resolves object nor starts an inheritance cycle.
    #[cfg(test)]
    fn incomplete() -> Self {
        Self(Box::default())
    }

    fn static_error_details(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class_literal: StaticClassLiteral<'db>,
        class: ClassType<'db>,
        original_bases: &[Type<'db>],
        resolved_bases: &[ClassBase<'db>],
    ) -> Result<Self, StaticMroError<'db>> {
        match error::static_error_details_with(
            db,
            env,
            class_literal,
            class,
            original_bases,
            resolved_bases,
            &construction::InlineStaticMroEffects::new(db),
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    pub(super) fn from_error(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
    ) -> Self {
        Self::from_error_with_object(class, ClassBase::object(db, env))
    }

    pub(in crate::types) fn from_error_with_object(
        class: ClassType<'db>,
        object: ClassBase<'db>,
    ) -> Self {
        Self::from([ClassBase::Class(class), ClassBase::unknown(), object])
    }

    /// Attempt to resolve the MRO of a dynamic class (created via `type(name, bases, dict)`).
    ///
    /// Uses C3 linearization when possible, returning an error if the MRO cannot be resolved.
    pub(super) fn of_dynamic_class(
        db: &'db dyn Db,
        dynamic: DynamicClassLiteral<'db>,
    ) -> Result<Self, DynamicMroError<'db>> {
        match dynamic::dynamic_mro_with(db, dynamic, &InlineMroRootEffects::new(db)) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Compute the MRO of a dynamic enum (created via the functional `Enum()`/`StrEnum()` API).
    ///
    /// Uses C3 linearization to correctly handle the optional `type=` mixin parameter.
    /// For example, `Enum("Http", {"OK": 200}, type=int)` is equivalent to
    /// `class Http(int, Enum)` at runtime, so the MRO must be C3-linearized from
    /// both bases to produce `[Http, int, Enum, object]`
    pub(super) fn of_dynamic_enum(
        db: &'db dyn Db,
        dynamic_enum: DynamicEnumLiteral<'db>,
    ) -> Result<Self, DynamicMroError<'db>> {
        match dynamic::dynamic_enum_mro_with(db, dynamic_enum, &InlineMroRootEffects::new(db)) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }
}

impl<'db, const N: usize> From<[ClassBase<'db>; N]> for Mro<'db> {
    fn from(value: [ClassBase<'db>; N]) -> Self {
        Self(Box::from(value))
    }
}

impl<'db> From<Vec<ClassBase<'db>>> for Mro<'db> {
    fn from(value: Vec<ClassBase<'db>>) -> Self {
        Self(value.into_boxed_slice())
    }
}

impl<'db> Deref for Mro<'db> {
    type Target = [ClassBase<'db>];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'db> FromIterator<ClassBase<'db>> for Mro<'db> {
    fn from_iter<T: IntoIterator<Item = ClassBase<'db>>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// Iterator that yields elements of a class's MRO.
///
/// We avoid materialising the *full* MRO unless it is actually necessary:
/// - Materialising the full MRO is expensive
/// - We need to do it for every class in the code that we're checking, as we need to make sure
///   that there are no class definitions in the code we're checking that would cause an
///   exception to be raised at runtime. But the same does *not* necessarily apply for every class
///   in third-party and stdlib dependencies: we never emit diagnostics about non-first-party code.
/// - However, we *do* need to resolve attribute accesses on classes/instances from
///   third-party and stdlib dependencies. That requires iterating over the MRO of third-party/stdlib
///   classes, but not necessarily the *whole* MRO: often just the first element is enough.
///   Luckily we know that for any class `X`, the first element of `X`'s MRO will always be `X` itself.
///   We can therefore avoid resolving the full MRO for many third-party/stdlib classes while still
///   being faithful to the runtime semantics.
///
/// Even for first-party code, where we will have to resolve the MRO for every class we encounter,
/// loading the cached MRO comes with a certain amount of overhead, so it's best to avoid calling the
/// Salsa-tracked [`StaticClassLiteral::try_mro`] method unless it's absolutely necessary.
#[derive(Clone)]
pub(crate) struct MroIterator<'db> {
    db: &'db dyn Db,
    cursor: MroCursor<'db>,
}

impl<'db> MroIterator<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Self {
        Self {
            db,
            cursor: MroCursor::new(class, specialization),
        }
    }

    fn advance(&mut self, direction: MroDirection) -> Option<ClassBase<'db>> {
        match mro_next_sync(
            self.db,
            &mut self.cursor,
            direction,
            &InlineMroRootEffects::new(self.db),
        ) {
            Ok(next) => next,
            Err(never) => match never {},
        }
    }
}

impl<'db> Iterator for MroIterator<'db> {
    type Item = ClassBase<'db>;

    fn next(&mut self) -> Option<Self::Item> {
        self.advance(MroDirection::Forward)
    }
}

impl std::iter::FusedIterator for MroIterator<'_> {}

impl DoubleEndedIterator for MroIterator<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.advance(MroDirection::Reverse)
    }
}

/// Boxed in cached MRO results so successful MROs do not reserve space for failure details.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct StaticMroError<'db> {
    kind: StaticMroErrorKind<'db>,
    fallback_mro: Mro<'db>,
    #[cfg(feature = "experimental-analysis")]
    retirement_work: Option<usize>,
}

impl<'db> StaticMroError<'db> {
    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) fn retirement_work(&self) -> Option<usize> {
        self.retirement_work
    }

    /// Construct an MRO error of kind `InheritanceCycle`.
    #[cfg(test)]
    pub(super) fn cycle(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
    ) -> Self {
        StaticMroErrorKind::InheritanceCycle.into_mro_error(db, env, class)
    }

    pub(super) fn is_cycle(&self) -> bool {
        matches!(self.kind, StaticMroErrorKind::InheritanceCycle)
    }

    /// Return an [`StaticMroErrorKind`] variant describing why we could not resolve the MRO for this class.
    pub(super) fn reason(&self) -> &StaticMroErrorKind<'db> {
        &self.kind
    }

    /// Return the fallback MRO we should infer for this class during type inference
    /// (since accurate resolution of its "true" MRO was impossible)
    pub(in crate::types) fn fallback_mro(&self) -> &Mro<'db> {
        &self.fallback_mro
    }
}

/// Possible ways in which attempting to resolve the MRO of a statically-defined class might fail.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) enum StaticMroErrorKind<'db> {
    /// The class inherits from one or more invalid bases.
    ///
    /// To avoid excessive complexity in our implementation,
    /// we only permit classes to inherit from class-literal types,
    /// `Todo`, `Unknown` or `Any`. Anything else results in us
    /// emitting a diagnostic.
    ///
    /// This variant records the indices and types of class bases
    /// that we deem to be invalid. The indices are the indices of nodes
    /// in the bases list of the class's [`StmtClassDef`](ruff_python_ast::StmtClassDef) node.
    /// Each index is the index of a node representing an invalid base.
    InvalidBases(Box<[(usize, Type<'db>)]>),

    /// The class has one or more duplicate bases.
    /// See [`DuplicateBaseError`] for more details.
    DuplicateBases(Box<[DuplicateBaseError<'db>]>),

    /// The class uses PEP-695 parameters and also inherits from `Generic[]`.
    Pep695ClassWithGenericInheritance,

    /// A cycle was encountered resolving the class' bases.
    InheritanceCycle,

    /// The MRO is otherwise unresolvable through the C3-merge algorithm.
    ///
    /// See [`c3_merge`] for more details.
    UnresolvableMro {
        bases_list: Box<[Type<'db>]>,
        /// If the error can be resolved by moving a `Generic[]` base
        /// to the end of the MRO, this field indicates the index of
        /// the `Generic[]` base. This allows us to provide an
        /// autofix when the diagnostic is emitted.
        generic_index: Option<usize>,
    },
}

impl<'db> StaticMroErrorKind<'db> {
    fn into_mro_error(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
    ) -> StaticMroError<'db> {
        self.into_mro_error_with_object(class, ClassBase::object(db, env))
    }

    pub(in crate::types) fn into_mro_error_with_object(
        self,
        class: ClassType<'db>,
        object: ClassBase<'db>,
    ) -> StaticMroError<'db> {
        let fallback_mro = Mro::from_error_with_object(class, object);
        #[cfg(feature = "experimental-analysis")]
        let retirement_work = {
            // Cache the nested ownership count while constructing the error. Inspecting a
            // retained memo can then quote destruction without traversing its duplicate bases.
            let details = match &self {
                Self::InvalidBases(bases) => 1usize.checked_add(bases.len()),
                Self::DuplicateBases(bases) => bases.iter().try_fold(1usize, |work, base| {
                    work.checked_add(3)?.checked_add(base.later_indices.len())
                }),
                Self::UnresolvableMro { bases_list, .. } => 2usize.checked_add(bases_list.len()),
                Self::Pep695ClassWithGenericInheritance | Self::InheritanceCycle => Some(1),
            };
            details.and_then(|work| work.checked_add(3)?.checked_add(fallback_mro.0.len()))
        };
        StaticMroError {
            kind: self,
            fallback_mro,
            #[cfg(feature = "experimental-analysis")]
            retirement_work,
        }
    }
}

/// Error recording the fact that a class definition was found to have duplicate bases.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct DuplicateBaseError<'db> {
    /// The base that is duplicated in the class's bases list.
    pub(super) duplicate_base: ClassBase<'db>,
    /// The index of the first occurrence of the base in the class's bases list.
    pub(super) first_index: usize,
    /// The indices of the base's later occurrences in the class's bases list.
    pub(super) later_indices: Box<[usize]>,
}

/// Implementation of the [C3-merge algorithm] for calculating a Python class's
/// [method resolution order].
///
/// [C3-merge algorithm]: https://docs.python.org/3/howto/mro.html#python-2-3-mro
/// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
fn c3_merge<'db>(db: &'db dyn Db, sequences: Vec<VecDeque<ClassBase<'db>>>) -> Option<Mro<'db>> {
    match c3::c3_merge_sync(db, sequences, &c3::InlineC3Effects) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}

/// Error for dynamic class MRO computation with fallback MRO.
///
/// Separate from [`StaticMroError`] because dynamic classes can only have a subset of MRO errors.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct DynamicMroError<'db> {
    kind: DynamicMroErrorKind<'db>,
    fallback_mro: Mro<'db>,
}

impl<'db> DynamicMroError<'db> {
    /// Return the error kind describing why we could not resolve the MRO.
    pub(crate) fn reason(&self) -> &DynamicMroErrorKind<'db> {
        &self.kind
    }

    /// Return the fallback MRO to use for type inference.
    fn fallback_mro(&self) -> &Mro<'db> {
        &self.fallback_mro
    }
}

/// Error kinds for dynamic class MRO computation.
///
/// These mirror the relevant variants from `MroErrorKind` for static classes.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum DynamicMroErrorKind<'db> {
    /// The class inherits from one or more invalid bases.
    ///
    /// Similar to `StaticMroErrorKind::InvalidBases`, this records the indices
    /// and types of bases that could not be converted to valid class bases.
    InvalidBases(Box<[(usize, Type<'db>)]>),

    /// A cycle was encountered resolving the class' bases.
    InheritanceCycle,

    /// The class has duplicate bases in its bases tuple.
    DuplicateBases(Box<[ClassBase<'db>]>),

    /// The MRO is unresolvable through the C3-merge algorithm.
    UnresolvableMro,
}

impl<'db> DynamicMroErrorKind<'db> {
    fn into_error(self, fallback_mro: Mro<'db>) -> DynamicMroError<'db> {
        DynamicMroError {
            kind: self,
            fallback_mro,
        }
    }
}
