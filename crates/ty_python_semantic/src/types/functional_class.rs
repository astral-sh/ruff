//! Types for classes created via the functional form `type(name, bases, dict)`.

use std::collections::VecDeque;

use crate::Db;
use crate::place::PlaceAndQualifiers;
use crate::types::class::{ClassMemberResult, InstanceMemberResult, MroLookup};
use crate::types::class_base::ClassBase;
use crate::types::mro::{Mro, c3_merge};
use crate::types::{ClassType, KnownClass, MemberLookupPolicy, SubclassOfType, Type};
use ruff_python_ast::name::Name;

/// A class created via the functional form: a three-argument `type()` call.
///
/// For example:
/// ```python
/// Foo = type("Foo", (Base,), {"attr": 1})
/// ```
///
/// The type of `Foo` would be `type[Foo]` where `Foo` is a `FunctionalClassType` with:
/// - name: "Foo"
/// - bases: [Base]
///
/// This is called "functional" because it mirrors the terminology used for `NamedTuple`
/// and `TypedDict`, where the "functional form" means creating via a function call
/// rather than a class statement.
///
/// # Limitations
///
/// TODO: Attributes from the namespace dict (third argument to `type()`) are not tracked.
/// This matches Pyright's behavior. For example:
/// ```python
/// Foo = type("Foo", (), {"attr": 42})
/// Foo().attr  # Error: no attribute 'attr'
/// ```
/// Supporting namespace dict attributes would require parsing dict literals and tracking
/// the attribute types, similar to how TypedDict handles its fields.
///
/// # Salsa interning
///
/// Two `type()` calls with the same name and bases produce the same `FunctionalClassType`
/// instance. This matches Pyright's behavior where:
/// ```python
/// Foo1 = type("Foo", (Base,), {})
/// Foo2 = type("Foo", (Base,), {})
/// # Foo1 and Foo2 have the same type: type[Foo]
/// ```
#[salsa::interned(debug, heap_size = ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct FunctionalClassType<'db> {
    /// The name of the class (from the first argument to `type()`).
    #[returns(ref)]
    pub name: Name,

    /// The base classes (from the second argument to `type()`).
    /// This is stored as a boxed slice for efficiency.
    /// Uses `ClassBase` to allow functional classes to inherit from other functional classes.
    #[returns(ref)]
    pub bases: Box<[ClassBase<'db>]>,
}

impl get_size2::GetSize for FunctionalClassType<'_> {}

impl<'db> FunctionalClassType<'db> {
    /// Returns an instance type for this functional class.
    pub(crate) fn to_instance(self, _db: &'db dyn Db) -> Type<'db> {
        Type::FunctionalInstance(self)
    }

    /// Check if this functional class is a subclass of another class.
    ///
    /// A functional class is a subclass of `other` if any of its bases is a subclass of `other`.
    pub(crate) fn is_subclass_of(self, db: &'db dyn Db, other: ClassType<'db>) -> bool {
        // A functional class is always a subclass of object.
        if other.is_object(db) {
            return true;
        }

        // Check if any base is a subclass of `other`.
        self.bases(db).iter().any(|base| match base {
            ClassBase::Class(class) => class.is_subclass_of(db, other),
            ClassBase::FunctionalClass(functional) => functional.is_subclass_of(db, other),
            ClassBase::Dynamic(_) => false,
            ClassBase::Protocol | ClassBase::Generic | ClassBase::TypedDict => false,
        })
    }

    /// Check if this functional class is a subclass of another functional class.
    ///
    /// Returns true if `self` is the same as `other`, or if any of `self`'s bases
    /// is a subclass of `other`.
    pub(crate) fn is_subclass_of_functional(self, db: &'db dyn Db, other: Self) -> bool {
        // Same functional class (via salsa interning).
        if self == other {
            return true;
        }

        // Check if any base is a subclass of `other`.
        self.bases(db).iter().any(|base| match base {
            ClassBase::FunctionalClass(functional) => {
                functional.is_subclass_of_functional(db, other)
            }
            ClassBase::Class(_) | ClassBase::Dynamic(_) => false,
            ClassBase::Protocol | ClassBase::Generic | ClassBase::TypedDict => false,
        })
    }

    /// Get the metaclass of this functional class.
    ///
    /// Derives the metaclass from base classes: finds the most derived metaclass
    /// that is a subclass of all other base metaclasses.
    ///
    /// See <https://docs.python.org/3/reference/datamodel.html#determining-the-appropriate-metaclass>
    pub(crate) fn metaclass(self, db: &'db dyn Db) -> Type<'db> {
        let bases = self.bases(db);

        // If no bases, metaclass is `type`
        if bases.is_empty() {
            return KnownClass::Type.to_instance(db);
        }

        // Start with the first base's metaclass as the candidate
        let mut candidate = bases[0].metaclass(db);

        // Reconcile with other bases' metaclasses
        for base in &bases[1..] {
            let base_metaclass = base.metaclass(db);

            // Get the ClassType for comparison
            let Some(candidate_class) = candidate.to_class_type(db) else {
                // If candidate isn't a class type, keep it as is
                continue;
            };
            let Some(base_metaclass_class) = base_metaclass.to_class_type(db) else {
                continue;
            };

            // If base's metaclass is more derived, use it
            if base_metaclass_class.is_subclass_of(db, candidate_class) {
                candidate = base_metaclass;
                continue;
            }

            // If candidate is already more derived, keep it
            if candidate_class.is_subclass_of(db, base_metaclass_class) {
                continue;
            }

            // Conflict: neither metaclass is a subclass of the other.
            // Python raises `TypeError: metaclass conflict` at runtime.
            // Return unknown to avoid cascading errors.
            return SubclassOfType::subclass_of_unknown();
        }

        candidate
    }

    /// Iterate over the MRO of this functional class using C3 linearization.
    ///
    /// The MRO includes the functional class itself as the first element, followed
    /// by the merged base class MROs (consistent with `ClassType::iter_mro`).
    ///
    /// If the MRO cannot be computed (e.g., due to inconsistent ordering), falls back
    /// to iterating over base MROs sequentially with deduplication.
    pub(crate) fn iter_mro(self, db: &'db dyn Db) -> impl Iterator<Item = ClassBase<'db>> + 'db {
        FunctionalMroIterator::new(db, self)
    }

    /// Compute the MRO as a fallback when `try_mro` fails.
    ///
    /// Iterates over base MROs sequentially with deduplication. This is used
    /// when there's a duplicate base or C3 linearization fails.
    fn compute_fallback_mro(self, db: &'db dyn Db) -> Mro<'db> {
        let mut result = vec![ClassBase::FunctionalClass(self)];
        let mut seen = std::collections::HashSet::new();
        seen.insert(ClassBase::FunctionalClass(self));

        for base in self.bases(db) {
            for item in base.mro(db, None) {
                if seen.insert(item) {
                    result.push(item);
                }
            }
        }

        Mro::from(result)
    }

    /// Look up an instance member by iterating through the MRO.
    pub(crate) fn instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        match MroLookup::new(db, self.iter_mro(db)).instance_member(name) {
            InstanceMemberResult::Done(result) => result,
            InstanceMemberResult::TypedDict => {
                // Simplified `TypedDict` handling without type mapping
                KnownClass::TypedDictFallback
                    .to_instance(db)
                    .instance_member(db, name)
            }
        }
    }

    /// Look up a class-level member by iterating through the MRO.
    ///
    /// Uses `MroLookup` with:
    /// - No inherited generic context (functional classes aren't generic)
    /// - `is_self_object = false` (functional classes are never `object`)
    pub(crate) fn class_member(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        let result = MroLookup::new(db, self.iter_mro(db)).class_member(
            name, policy, None,  // No inherited generic context
            false, // Functional classes are never `object`
        );

        match result {
            ClassMemberResult::Done { .. } => result.finalize(db),
            ClassMemberResult::TypedDict => {
                // Simplified `TypedDict` handling without type mapping
                KnownClass::TypedDictFallback
                    .to_class_literal(db)
                    .find_name_in_mro_with_policy(db, name, policy)
                    .expect("Will return Some() when called on class literal")
            }
        }
    }

    /// Try to compute the MRO for this functional class.
    ///
    /// Returns `Ok(Mro)` if successful, or `Err(FunctionalMroError)` if there's
    /// an error (duplicate bases or C3 linearization failure).
    pub(crate) fn try_mro(self, db: &'db dyn Db) -> Result<Mro<'db>, FunctionalMroError<'db>> {
        let bases = self.bases(db);

        // Check for duplicate bases first
        let mut seen = std::collections::HashSet::new();
        let mut duplicates = Vec::new();
        for base in bases {
            if !seen.insert(*base) {
                duplicates.push(*base);
            }
        }
        if !duplicates.is_empty() {
            return Err(FunctionalMroError::DuplicateBases(
                duplicates.into_boxed_slice(),
            ));
        }

        // Compute MRO using C3 linearization
        let mro_bases = if bases.is_empty() {
            // Empty bases: MRO is just `object`.
            vec![ClassBase::object(db)]
        } else if bases.len() == 1 {
            // Single base: MRO is just that base's MRO.
            bases[0].mro(db, None).collect()
        } else {
            // Multiple bases: use C3 merge algorithm.
            let mut seqs: Vec<VecDeque<ClassBase<'db>>> = Vec::with_capacity(bases.len() + 1);

            // Add each base's MRO.
            for base in bases {
                seqs.push(base.mro(db, None).collect());
            }

            // Add the list of bases in order.
            seqs.push(bases.iter().copied().collect());

            c3_merge(seqs)
                .map(|mro| mro.iter().copied().collect())
                .ok_or(FunctionalMroError::UnresolvableMro)?
        };

        let mut result = vec![ClassBase::FunctionalClass(self)];
        result.extend(mro_bases);
        Ok(Mro::from(result))
    }
}

#[salsa::tracked]
impl<'db> FunctionalClassType<'db> {
    /// Compute and cache the MRO for this functional class.
    ///
    /// Uses C3 linearization when possible, falling back to sequential iteration
    /// with deduplication when there's an error (duplicate bases or C3 merge failure).
    #[salsa::tracked(heap_size = ruff_memory_usage::heap_size)]
    pub(crate) fn mro(self, db: &'db dyn Db) -> Mro<'db> {
        self.try_mro(db)
            .unwrap_or_else(|_| self.compute_fallback_mro(db))
    }
}

/// Iterator over the MRO of a functional class.
///
/// Uses the cached MRO from [`FunctionalClassType::mro`].
struct FunctionalMroIterator<'db> {
    /// The cached MRO
    mro: Mro<'db>,
    /// Current index into the MRO
    index: usize,
}

impl<'db> FunctionalMroIterator<'db> {
    fn new(db: &'db dyn Db, functional_class: FunctionalClassType<'db>) -> Self {
        FunctionalMroIterator {
            mro: functional_class.mro(db),
            index: 0,
        }
    }
}

impl<'db> Iterator for FunctionalMroIterator<'db> {
    type Item = ClassBase<'db>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.mro.len() {
            let item = self.mro[self.index];
            self.index += 1;
            Some(item)
        } else {
            None
        }
    }
}

impl std::iter::FusedIterator for FunctionalMroIterator<'_> {}

/// Error kinds for functional class MRO computation.
///
/// These mirror the relevant variants from `MroErrorKind` for regular classes.
#[derive(Debug, Clone)]
pub(crate) enum FunctionalMroError<'db> {
    /// The class has duplicate bases in its bases tuple.
    DuplicateBases(Box<[ClassBase<'db>]>),

    /// The MRO is unresolvable through the C3-merge algorithm.
    UnresolvableMro,
}
