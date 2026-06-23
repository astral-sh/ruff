//! Compact equality reasoning for values from the same enum class.

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use ty_python_core::Truthiness;

use crate::types::{
    EnumClassLiteral, EnumComplementType, EnumLiteralType, IntersectionBuilder, IntersectionType,
    LiteralValueTypeKind, Type, UnionBuilder,
};
use crate::{Db, FxOrderSet};

use super::{ComparisonBranch, ComparisonOperator, ComparisonResult, KnownComparisonSemantics};

/// Compare two compact domains from the same enum without expanding their members.
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

enum EnumValueSetMembers<'db> {
    All,
    One(&'db Name),
    Included(FxOrderSet<&'db Name>),
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
                let LiteralValueTypeKind::Enum(literal) = literal.kind() else {
                    return None;
                };
                let enum_class = literal.enum_class_literal(db);
                let name = enum_class.resolve_member(db, literal.name(db))?;
                Self {
                    enum_class,
                    members: EnumValueSetMembers::One(name),
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

    fn from_union(db: &'db dyn Db, elements: &[Type<'db>]) -> Option<Self> {
        let mut enum_class = None;
        let mut included = FxOrderSet::default();
        for element in elements {
            let value_set = Self::from_type(db, *element)?;
            if let Some(enum_class) = enum_class
                && enum_class != value_set.enum_class
            {
                return None;
            }
            enum_class = Some(value_set.enum_class);
            match value_set.members {
                EnumValueSetMembers::One(name) => {
                    included.insert(name);
                }
                EnumValueSetMembers::Included(names) => included.extend(names),
                EnumValueSetMembers::All | EnumValueSetMembers::AllExcept(_) => return None,
            }
        }

        let enum_class = enum_class?;
        let members = if included.len() == 1 {
            EnumValueSetMembers::One(included.into_iter().next()?)
        } else {
            EnumValueSetMembers::Included(included)
        };
        Some(Self {
            enum_class,
            members,
        })
    }

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
            EnumValueSetMembers::One(_) => 1,
            EnumValueSetMembers::Included(names) => names.len(),
            EnumValueSetMembers::AllExcept(complement) => {
                self.enum_class.member_count(db) - complement.excluded_names(db).len()
            }
        }
    }

    fn is_closed(&self, members_are_exhaustive: bool) -> bool {
        members_are_exhaustive
            || matches!(
                self.members,
                EnumValueSetMembers::One(_) | EnumValueSetMembers::Included(_)
            )
    }

    fn is_singleton(&self, db: &'db dyn Db, members_are_exhaustive: bool) -> bool {
        self.member_count(db) == 1 && self.is_closed(members_are_exhaustive)
    }

    fn overlaps(&self, other: &Self, db: &'db dyn Db) -> bool {
        debug_assert_eq!(self.enum_class, other.enum_class);
        match (&self.members, &other.members) {
            (EnumValueSetMembers::All, _) | (_, EnumValueSetMembers::All) => true,
            (EnumValueSetMembers::One(left), EnumValueSetMembers::One(right)) => left == right,
            (EnumValueSetMembers::One(name), EnumValueSetMembers::Included(names))
            | (EnumValueSetMembers::Included(names), EnumValueSetMembers::One(name)) => {
                names.contains(name)
            }
            (EnumValueSetMembers::Included(left), EnumValueSetMembers::Included(right)) => {
                let (smaller, larger) = if left.len() < right.len() {
                    (left, right)
                } else {
                    (right, left)
                };
                smaller.iter().any(|name| larger.contains(name))
            }
            (EnumValueSetMembers::One(name), EnumValueSetMembers::AllExcept(complement))
            | (EnumValueSetMembers::AllExcept(complement), EnumValueSetMembers::One(name)) => {
                !complement.excluded_names(db).contains(*name)
            }
            (EnumValueSetMembers::Included(names), EnumValueSetMembers::AllExcept(complement))
            | (EnumValueSetMembers::AllExcept(complement), EnumValueSetMembers::Included(names)) => {
                names
                    .iter()
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
            EnumValueSetMembers::One(name) => {
                Type::enum_literal(EnumLiteralType::new(db, self.enum_class, (*name).clone()))
            }
            EnumValueSetMembers::Included(names) => names
                .iter()
                .fold(UnionBuilder::new(db), |builder, name| {
                    builder.add(Type::enum_literal(EnumLiteralType::new(
                        db,
                        self.enum_class,
                        (*name).clone(),
                    )))
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

    fn singleton_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        if self.member_count(db) != 1 {
            return None;
        }
        let name = match &self.members {
            EnumValueSetMembers::All => self.enum_class.member_names(db).next()?,
            EnumValueSetMembers::One(name) => name,
            EnumValueSetMembers::Included(names) => names.first()?,
            EnumValueSetMembers::AllExcept(complement) => self
                .enum_class
                .member_names(db)
                .find(|name| !complement.excluded_names(db).contains(*name))?,
        };
        Some(Type::enum_literal(EnumLiteralType::new(
            db,
            self.enum_class,
            (*name).clone(),
        )))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
enum EnumComparisonKeys {
    Distinct,
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

fn enum_members_have_distinct_value_keys<'db>(
    db: &'db dyn Db,
    enum_class: EnumClassLiteral<'db>,
) -> bool {
    let mut keys = FxHashSet::default();
    enum_class.members(db).iter().all(|(_, value)| {
        let key = match value.as_literal_value_kind() {
            Some(LiteralValueTypeKind::Bool(value)) => Type::int_literal(i64::from(value)),
            Some(
                LiteralValueTypeKind::Int(_)
                | LiteralValueTypeKind::String(_)
                | LiteralValueTypeKind::Bytes(_),
            ) => *value,
            Some(LiteralValueTypeKind::LiteralString | LiteralValueTypeKind::Enum(_)) | None => {
                return false;
            }
        };
        keys.insert(key)
    })
}
