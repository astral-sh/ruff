//! Compact equality reasoning for values from the same enum class.

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use ty_python_core::Truthiness;

use crate::types::literal::IntLiteralType;
use crate::types::{
    EnumClassLiteral, EnumComplementType, EnumLiteralType, IntersectionBuilder, IntersectionType,
    LiteralValueType, LiteralValueTypeKind, Type, UnionBuilder,
};
use crate::{Db, FxOrderMap, FxOrderSet};

use super::{ComparisonBranch, ComparisonOperator, ComparisonResult, KnownComparisonSemantics};

/// Compare two compact domains from the same enum without expanding their members.
///
/// Any narrowing constraint produced here contains only enum-membership facts. In particular,
/// equality never transfers gradual or nominal intersection state from one operand to the other.
pub(super) fn evaluate_same_enum_domains<'db>(
    db: &'db dyn Db,
    target: Type<'db>,
    other: Type<'db>,
    branch: ComparisonBranch,
    operator: ComparisonOperator,
) -> Option<ComparisonResult<'db>> {
    let comparison = SameEnumComparison::from_types(db, target, other, operator)?;
    match comparison.truthiness(db, operator)? {
        Truthiness::AlwaysTrue => Some(ComparisonResult::AlwaysTrue),
        Truthiness::AlwaysFalse => Some(ComparisonResult::AlwaysFalse),
        Truthiness::Ambiguous if !comparison.supports_domain_narrowing() => {
            Some(ComparisonResult::Ambiguous)
        }
        Truthiness::Ambiguous if operator.condition_expects_equality(branch) => Some(
            ComparisonResult::CanNarrow(comparison.right.restriction_type(db)),
        ),
        Truthiness::Ambiguous => Some(comparison.right.singleton_type(db).map_or(
            ComparisonResult::Ambiguous,
            |singleton| {
                ComparisonResult::CanNarrow(
                    IntersectionBuilder::new(db)
                        .add_positive(comparison.left.restriction_type(db))
                        .add_negative(singleton)
                        .build(),
                )
            },
        )),
    }
}

/// The result of comparing two compact domains from the same enum.
pub(in crate::types) enum EnumComparison {
    /// The comparison method is modeled and has the given truthiness.
    Known(Truthiness),
    /// A custom or otherwise unmodeled comparison method must be called directly.
    Unmodeled,
}

/// Compare two compact domains from the same enum without expanding either operand.
///
/// `None` means that an operand is not structurally an enum domain.
pub(in crate::types) fn compact_enum_comparison<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    is_equality: bool,
) -> Option<EnumComparison> {
    let operator = if is_equality {
        ComparisonOperator::Equality
    } else {
        ComparisonOperator::Inequality
    };
    let comparison = SameEnumComparison::from_types(db, left, right, operator)?;
    Some(
        comparison
            .truthiness(db, operator)
            .map_or(EnumComparison::Unmodeled, EnumComparison::Known),
    )
}

/// Return the constraint established by membership in an exact set of members from the same enum.
pub(in crate::types) fn enum_membership_constraint<'db>(
    db: &'db dyn Db,
    target: Type<'db>,
    members: Type<'db>,
    is_positive: bool,
) -> Option<Type<'db>> {
    let comparison =
        SameEnumComparison::from_types(db, target, members, ComparisonOperator::Equality)?;
    if !comparison.supports_domain_narrowing() {
        return None;
    }

    let members = comparison.right.restriction_type(db);
    if is_positive {
        Some(members)
    } else {
        Some(
            IntersectionBuilder::new(db)
                .add_positive(comparison.left.restriction_type(db))
                .add_negative(members)
                .build(),
        )
    }
}

/// Two non-empty value domains from the same enum and the semantics used to compare them.
///
/// Keeping the operands compact avoids constructing and pairwise comparing unions of every
/// declared member.
struct SameEnumComparison<'db> {
    left: EnumValueSet<'db>,
    right: EnumValueSet<'db>,
    profile: EnumComparisonProfile,
}

impl<'db> SameEnumComparison<'db> {
    fn from_types(
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        operator: ComparisonOperator,
    ) -> Option<Self> {
        let left = EnumValueSet::from_type(db, left)?;
        let right = EnumValueSet::from_type(db, right)?;
        if left.enum_class != right.enum_class {
            return None;
        }
        let enum_class = left.enum_class;

        Some(Self {
            left,
            right,
            profile: enum_comparison_profile(db, enum_class, operator),
        })
    }

    /// Return `None` only when comparison behavior is custom or otherwise unmodeled.
    fn truthiness(&self, db: &'db dyn Db, operator: ComparisonOperator) -> Option<Truthiness> {
        let comparison_keys = self.profile.comparison_keys?;
        let members_are_exhaustive = self.profile.members_are_exhaustive;
        let domains_are_closed = self.left.is_closed(members_are_exhaustive)
            && self.right.is_closed(members_are_exhaustive);
        let equality = if domains_are_closed
            && comparison_keys == EnumComparisonKeys::Distinct
            && !self.left.overlaps(&self.right, db)
        {
            Truthiness::AlwaysFalse
        } else if self.left.is_singleton(db, members_are_exhaustive)
            && self.right.is_singleton(db, members_are_exhaustive)
            && self.left.overlaps(&self.right, db)
        {
            Truthiness::AlwaysTrue
        } else {
            Truthiness::Ambiguous
        };

        Some(equality.negate_if(operator == ComparisonOperator::Inequality))
    }

    /// Return whether equality can soundly transfer the right-hand enum restriction to the left.
    ///
    /// The right domain must be closed, and the left must either be closed as well or use identity
    /// comparison, for which undeclared runtime members cannot equal a declared member.
    fn supports_domain_narrowing(&self) -> bool {
        matches!(
            self.profile.comparison_keys,
            Some(EnumComparisonKeys::Distinct)
        ) && self.right.is_closed(self.profile.members_are_exhaustive)
            && (self.left.is_closed(self.profile.members_are_exhaustive)
                || self.profile.members_compare_by_identity)
    }
}

/// The enum-member values represented by an operand, excluding non-enum intersection state.
///
/// This is an upper bound on the operand's enum values. Gradual and nominal rest components can
/// make the operand more specific, but they must never be transferred to the other operand by an
/// equality constraint.
struct EnumValueSet<'db> {
    enum_class: EnumClassLiteral<'db>,
    members: EnumValueSetMembers<'db>,
}

/// Compact representation of the member names admitted by an [`EnumValueSet`].
enum EnumValueSetMembers<'db> {
    /// The entire enum domain, including undeclared runtime values when the enum is open.
    All,
    /// One canonical member name after resolving aliases, and whether its literal is promotable.
    One { name: &'db Name, promotable: bool },
    /// An exact set of canonical member names and their literal promotability.
    Included(FxOrderMap<&'db Name, bool>),
    /// The enum domain except for the declared members excluded by an enum complement.
    AllExcept(EnumComplementType<'db>),
}

impl<'db> EnumValueSet<'db> {
    /// Extract only structural enum membership facts from `ty`.
    ///
    /// This deliberately does not use subtyping: a `NewType` over an enum is a subtype of the
    /// enum but remains disjoint from the enum's literal members.
    fn from_type(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        let value_set = match ty.resolve_type_alias(db) {
            Type::LiteralValue(literal) => {
                let LiteralValueTypeKind::Enum(enum_literal) = literal.kind() else {
                    return None;
                };
                let enum_class = enum_literal.enum_class_literal(db);
                let name = enum_class.resolve_member(db, enum_literal.name(db))?;
                Self {
                    enum_class,
                    members: EnumValueSetMembers::One {
                        name,
                        promotable: literal.is_promotable(),
                    },
                }
            }
            Type::NominalInstance(instance) => Self {
                enum_class: instance.class_literal(db).into_enum_class(db)?,
                members: EnumValueSetMembers::All,
            },
            Type::EnumComplement(complement) => Self {
                enum_class: complement.enum_class_literal(db),
                members: EnumValueSetMembers::AllExcept(complement),
            },
            Type::Union(union) => Self::from_union(db, union.elements(db))?,
            Type::Intersection(intersection) => Self::from_intersection(db, intersection)?,
            _ => return None,
        };
        (value_set.member_count(db) > 0).then_some(value_set)
    }

    /// Extract an exact included-member set from a union of enum domains.
    ///
    /// Whole-domain and complement arms are rejected because they are not exact included sets.
    fn from_union(db: &'db dyn Db, elements: &[Type<'db>]) -> Option<Self> {
        let mut enum_class = None;
        let mut included = FxOrderMap::default();
        for element in elements {
            let value_set = Self::from_type(db, *element)?;
            if let Some(enum_class) = enum_class
                && enum_class != value_set.enum_class
            {
                return None;
            }
            enum_class = Some(value_set.enum_class);
            match value_set.members {
                EnumValueSetMembers::One { name, promotable } => {
                    Self::insert_member(&mut included, name, promotable);
                }
                EnumValueSetMembers::Included(members) => {
                    for (name, promotable) in members {
                        Self::insert_member(&mut included, name, promotable);
                    }
                }
                EnumValueSetMembers::All | EnumValueSetMembers::AllExcept(_) => return None,
            }
        }

        let enum_class = enum_class?;
        let members = if included.len() == 1 {
            let (name, promotable) = included.into_iter().next()?;
            EnumValueSetMembers::One { name, promotable }
        } else {
            EnumValueSetMembers::Included(included)
        };
        Some(Self {
            enum_class,
            members,
        })
    }

    /// Insert a member, preserving unpromotable literal provenance if either occurrence has it.
    fn insert_member(
        included: &mut FxOrderMap<&'db Name, bool>,
        name: &'db Name,
        promotable: bool,
    ) {
        match included.entry(name) {
            ordermap::map::Entry::Vacant(entry) => {
                entry.insert(promotable);
            }
            ordermap::map::Entry::Occupied(mut entry) => {
                *entry.get_mut() &= promotable;
            }
        }
    }

    /// Extract the enum restriction while discarding unrelated positive intersection state.
    fn from_intersection(db: &'db dyn Db, intersection: IntersectionType<'db>) -> Option<Self> {
        if let Some(complement) = intersection.enum_complement(db) {
            return Self::from_type(db, Type::EnumComplement(complement));
        }

        // Other intersection components can only reduce the represented enum values. Ignoring
        // them therefore preserves a safe upper bound without transferring them during narrowing.
        let mut value_sets = intersection
            .positive(db)
            .iter()
            .filter_map(|positive| Self::from_type(db, *positive));
        let value_set = value_sets.next()?;
        value_sets
            .all(|other| other.enum_class == value_set.enum_class)
            .then_some(value_set)
    }

    fn member_count(&self, db: &'db dyn Db) -> usize {
        match &self.members {
            EnumValueSetMembers::All => self.enum_class.member_count(db),
            EnumValueSetMembers::One { .. } => 1,
            EnumValueSetMembers::Included(names) => names.len(),
            EnumValueSetMembers::AllExcept(complement) => {
                self.enum_class.member_count(db) - complement.excluded_names(db).len()
            }
        }
    }

    /// Return whether this set excludes every value not named by its representation.
    ///
    /// A whole-domain or complement representation is not closed for an enum that can create
    /// undeclared members at runtime.
    fn is_closed(&self, members_are_exhaustive: bool) -> bool {
        members_are_exhaustive
            || matches!(
                self.members,
                EnumValueSetMembers::One { .. } | EnumValueSetMembers::Included(_)
            )
    }

    fn is_singleton(&self, db: &'db dyn Db, members_are_exhaustive: bool) -> bool {
        self.member_count(db) == 1 && self.is_closed(members_are_exhaustive)
    }

    fn overlaps(&self, other: &Self, db: &'db dyn Db) -> bool {
        debug_assert_eq!(self.enum_class, other.enum_class);
        match (&self.members, &other.members) {
            (EnumValueSetMembers::All, _) | (_, EnumValueSetMembers::All) => true,
            (
                EnumValueSetMembers::One { name: left, .. },
                EnumValueSetMembers::One { name: right, .. },
            ) => left == right,
            (EnumValueSetMembers::One { name, .. }, EnumValueSetMembers::Included(names))
            | (EnumValueSetMembers::Included(names), EnumValueSetMembers::One { name, .. }) => {
                names.contains_key(name)
            }
            (EnumValueSetMembers::Included(left), EnumValueSetMembers::Included(right)) => {
                let (smaller, larger) = if left.len() < right.len() {
                    (left, right)
                } else {
                    (right, left)
                };
                smaller.keys().any(|name| larger.contains_key(name))
            }
            (EnumValueSetMembers::One { name, .. }, EnumValueSetMembers::AllExcept(complement))
            | (EnumValueSetMembers::AllExcept(complement), EnumValueSetMembers::One { name, .. }) => {
                !complement.excluded_names(db).contains(*name)
            }
            (EnumValueSetMembers::Included(names), EnumValueSetMembers::AllExcept(complement))
            | (EnumValueSetMembers::AllExcept(complement), EnumValueSetMembers::Included(names)) => {
                names
                    .keys()
                    .any(|name| !complement.excluded_names(db).contains(*name))
            }
            (EnumValueSetMembers::AllExcept(left), EnumValueSetMembers::AllExcept(right)) => {
                let left = left.excluded_names(db);
                let right = right.excluded_names(db);
                let excluded =
                    left.len() + right.iter().filter(|name| !left.contains(*name)).count();
                excluded < self.enum_class.member_count(db)
            }
        }
    }

    /// Reconstruct a constraint containing only this enum value restriction.
    fn restriction_type(&self, db: &'db dyn Db) -> Type<'db> {
        match &self.members {
            EnumValueSetMembers::All => self
                .enum_class
                .class_literal(db)
                .to_non_generic_instance(db),
            EnumValueSetMembers::One { name, promotable } => {
                self.member_type(db, name, *promotable)
            }
            EnumValueSetMembers::Included(members) => members
                .iter()
                .fold(UnionBuilder::new(db), |builder, (name, promotable)| {
                    builder.add(self.member_type(db, name, *promotable))
                })
                .build(),
            EnumValueSetMembers::AllExcept(complement) => {
                if complement.rest(db).is_empty() {
                    Type::EnumComplement(*complement)
                } else {
                    Type::EnumComplement(EnumComplementType::new(
                        db,
                        self.enum_class,
                        complement.excluded_names(db).clone(),
                        FxOrderSet::default(),
                    ))
                }
            }
        }
    }

    /// Reconstruct the only declared member left in this set.
    ///
    /// The caller must separately establish that the domain is closed before treating this as the
    /// operand's only possible runtime value.
    fn singleton_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        if self.member_count(db) != 1 {
            return None;
        }
        let (name, promotable) = match &self.members {
            EnumValueSetMembers::All => (self.enum_class.member_names(db).next()?, true),
            EnumValueSetMembers::One { name, promotable } => (*name, *promotable),
            EnumValueSetMembers::Included(members) => {
                let (name, promotable) = members.first()?;
                (*name, *promotable)
            }
            EnumValueSetMembers::AllExcept(complement) => (
                self.enum_class
                    .member_names(db)
                    .find(|name| !complement.excluded_names(db).contains(*name))?,
                true,
            ),
        };
        Some(self.member_type(db, name, promotable))
    }

    fn member_type(&self, db: &'db dyn Db, name: &Name, promotable: bool) -> Type<'db> {
        LiteralValueType::new(
            EnumLiteralType::new(db, self.enum_class, name.clone()),
            promotable,
        )
        .into()
    }
}

/// Whether distinct declared members are known to have distinct runtime comparison keys.
#[derive(Debug, Copy, Clone, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
enum EnumComparisonKeys {
    /// Different member names cannot compare equal.
    Distinct,
    /// Values are unknown or repeated, so different member names may compare equal.
    UnknownOrRepeated,
}

/// Class-wide facts required to compare enum value sets without member expansion.
#[derive(Debug, Copy, Clone, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
struct EnumComparisonProfile {
    members_are_exhaustive: bool,
    /// Whether distinct enum members compare by identity, including members created at runtime.
    members_compare_by_identity: bool,
    /// `None` means comparison behavior is custom or otherwise unmodeled.
    comparison_keys: Option<EnumComparisonKeys>,
}

/// Compute and cache the class-wide work needed for repeated comparisons of the same enum.
#[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
fn enum_comparison_profile<'db>(
    db: &'db dyn Db,
    enum_class: EnumClassLiteral<'db>,
    operator: ComparisonOperator,
) -> EnumComparisonProfile {
    let semantics = KnownComparisonSemantics::of_instance(
        db,
        enum_class.class_literal(db).to_non_generic_instance(db),
        operator,
    );
    let (comparison_keys, members_compare_by_identity) = match semantics {
        None => (None, false),
        Some(KnownComparisonSemantics::Object) => (Some(EnumComparisonKeys::Distinct), true),
        Some(
            KnownComparisonSemantics::Int
            | KnownComparisonSemantics::Str
            | KnownComparisonSemantics::Bytes,
        ) if enum_members_have_distinct_value_keys(db, enum_class) => {
            (Some(EnumComparisonKeys::Distinct), false)
        }
        Some(_) => (Some(EnumComparisonKeys::UnknownOrRepeated), false),
    };
    EnumComparisonProfile {
        members_are_exhaustive: enum_class.members_are_exhaustive(db),
        members_compare_by_identity,
        comparison_keys,
    }
}

/// Return whether every declared member has a unique modeled runtime comparison key.
///
/// Keys exclude literal metadata such as promotability, which does not affect runtime equality.
/// Boolean keys are normalized to integers because Python considers `False == 0` and `True == 1`.
fn enum_members_have_distinct_value_keys<'db>(
    db: &'db dyn Db,
    enum_class: EnumClassLiteral<'db>,
) -> bool {
    let mut keys = FxHashSet::default();
    enum_class.members(db).iter().all(|(_, value)| {
        let key = match value.as_literal_value_kind() {
            Some(LiteralValueTypeKind::Bool(value)) => {
                LiteralValueTypeKind::Int(IntLiteralType::from_i64(i64::from(value)))
            }
            Some(
                kind @ (LiteralValueTypeKind::Int(_)
                | LiteralValueTypeKind::String(_)
                | LiteralValueTypeKind::Bytes(_)),
            ) => kind,
            Some(LiteralValueTypeKind::LiteralString | LiteralValueTypeKind::Enum(_)) | None => {
                return false;
            }
        };
        keys.insert(key)
    })
}
