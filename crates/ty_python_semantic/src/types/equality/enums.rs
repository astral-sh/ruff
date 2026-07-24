//! Equality reasoning for enum value domains without expanding member unions.

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use ty_python_core::Truthiness;

use crate::types::literal::IntLiteralType;
use crate::types::{
    EnumClassLiteral, EnumComplementType, EnumLiteralType, IntersectionBuilder, IntersectionType,
    LiteralValueType, LiteralValueTypeKind, Type, UnionBuilder,
};
use crate::{Db, FxOrderMap, FxOrderSet};

use super::{
    ComparisonBranch, ComparisonOperator, ComparisonResult, KnownComparisonSemantics,
    enum_literal_value,
};

/// Compare two enum value domains without comparing every pair of members.
///
/// Any narrowing constraint produced here contains only enum-membership facts. In particular,
/// equality never transfers gradual or nominal intersection state from one operand to the other.
/// Same-class domains compare compact member sets directly, while comparisons spanning multiple
/// classes project their members onto runtime comparison keys.
pub(super) fn evaluate_enum_domains<'db>(
    db: &'db dyn Db,
    target: Type<'db>,
    other: Type<'db>,
    branch: ComparisonBranch,
    operator: ComparisonOperator,
) -> Option<ComparisonResult<'db>> {
    let target = EnumDomainSet::from_type(db, target)?;
    let other = EnumDomainSet::from_type(db, other)?;
    if let (Some(target), Some(other)) = (target.single(), other.single())
        && target.enum_class == other.enum_class
    {
        return SameEnumComparison::new(db, target.clone(), other.clone(), operator)
            .evaluate(db, branch, operator);
    }

    ProjectedEnumComparison::new(db, target, &other, operator)?.evaluate(db, branch, operator)
}

/// Two non-empty value domains from the same enum and the semantics used to compare them.
///
/// This representation avoids constructing and pairwise comparing unions of every declared
/// member.
struct SameEnumComparison<'db> {
    left: EnumValueSet<'db>,
    right: EnumValueSet<'db>,
    profile: SameEnumComparisonProfile,
}

impl<'db> SameEnumComparison<'db> {
    fn new(
        db: &'db dyn Db,
        left: EnumValueSet<'db>,
        right: EnumValueSet<'db>,
        operator: ComparisonOperator,
    ) -> Self {
        debug_assert_eq!(left.enum_class, right.enum_class);
        let enum_class = left.enum_class;

        Self {
            left,
            right,
            profile: same_enum_comparison_profile(db, enum_class, operator),
        }
    }

    /// Return `None` only when comparison behavior is custom or otherwise unmodeled.
    fn truthiness(&self, db: &'db dyn Db, operator: ComparisonOperator) -> Option<Truthiness> {
        let comparison_keys = self.profile.comparison_keys?;
        let members_are_exhaustive = self.profile.members_are_exhaustive;
        let domains_are_closed = self.left.is_closed(members_are_exhaustive)
            && self.right.is_closed(members_are_exhaustive);
        let equality = if domains_are_closed
            && comparison_keys == SameEnumComparisonKeys::Distinct
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

    fn evaluate(
        &self,
        db: &'db dyn Db,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> Option<ComparisonResult<'db>> {
        match self.truthiness(db, operator)? {
            Truthiness::AlwaysTrue => Some(ComparisonResult::AlwaysTrue),
            Truthiness::AlwaysFalse => Some(ComparisonResult::AlwaysFalse),
            Truthiness::Ambiguous if !self.supports_domain_narrowing() => {
                Some(ComparisonResult::Ambiguous)
            }
            Truthiness::Ambiguous if operator.condition_expects_equality(branch) => {
                Some(ComparisonResult::CanNarrow(self.right.restriction_type(db)))
            }
            Truthiness::Ambiguous => Some(self.right.singleton_type(db).map_or(
                ComparisonResult::Ambiguous,
                |singleton| {
                    ComparisonResult::CanNarrow(
                        IntersectionBuilder::new(db)
                            .add_positive(self.left.restriction_type(db))
                            .add_negative(singleton)
                            .build(),
                    )
                },
            )),
        }
    }

    /// Return whether equality can soundly transfer the right-hand enum restriction to the left.
    ///
    /// The right domain must be closed, and the left must either be closed as well or use identity
    /// comparison, for which undeclared runtime members cannot equal a declared member.
    fn supports_domain_narrowing(&self) -> bool {
        matches!(
            self.profile.comparison_keys,
            Some(SameEnumComparisonKeys::Distinct)
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
#[derive(Clone)]
struct EnumValueSet<'db> {
    enum_class: EnumClassLiteral<'db>,
    members: EnumValueSetMembers<'db>,
}

/// Compact representation of the member names admitted by an [`EnumValueSet`].
#[derive(Clone)]
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
    fn from_type(
        db: &'db dyn Db,
        ty: Type<'db>,
        active_types: &mut FxHashSet<Type<'db>>,
    ) -> Option<Self> {
        fn from_type_inner<'db>(
            db: &'db dyn Db,
            ty: Type<'db>,
            active_types: &mut FxHashSet<Type<'db>>,
        ) -> Option<EnumValueSet<'db>> {
            let value_set = match ty.resolve_type_alias(db) {
                Type::LiteralValue(literal) => {
                    let LiteralValueTypeKind::Enum(enum_literal) = literal.kind() else {
                        return None;
                    };
                    let enum_class = enum_literal.enum_class_literal(db);
                    let name = enum_class.resolve_member(db, enum_literal.name(db))?;
                    EnumValueSet {
                        enum_class,
                        members: EnumValueSetMembers::One {
                            name,
                            promotable: literal.is_promotable(),
                        },
                    }
                }
                Type::NominalInstance(instance) => EnumValueSet {
                    enum_class: instance.class_literal(db).into_enum_class(db)?,
                    members: EnumValueSetMembers::All,
                },
                Type::EnumComplement(complement) => EnumValueSet {
                    enum_class: complement.enum_class_literal(db),
                    members: EnumValueSetMembers::AllExcept(complement),
                },
                Type::Union(union) => {
                    EnumValueSet::from_union(db, union.elements(db), active_types)?
                }
                Type::Intersection(intersection) => {
                    EnumValueSet::from_intersection(db, intersection, active_types)?
                }
                _ => return None,
            };
            (value_set.member_count(db) > 0).then_some(value_set)
        }

        // A cycle prevents extracting a finite enum domain, so fall back to general comparison.
        if !active_types.insert(ty) {
            return None;
        }
        let value_set = from_type_inner(db, ty, active_types);
        active_types.remove(&ty);
        value_set
    }

    /// Extract an exact included-member set from a union of enum domains.
    ///
    /// Whole-domain and complement arms are rejected because they are not exact included sets.
    fn from_union(
        db: &'db dyn Db,
        elements: &[Type<'db>],
        active_types: &mut FxHashSet<Type<'db>>,
    ) -> Option<Self> {
        let mut enum_class = None;
        let mut included = FxOrderMap::default();
        for element in elements {
            let value_set = Self::from_type(db, *element, active_types)?;
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

        Self::from_included(enum_class?, included)
    }

    fn from_included(
        enum_class: EnumClassLiteral<'db>,
        included: FxOrderMap<&'db Name, bool>,
    ) -> Option<Self> {
        if included.is_empty() {
            return None;
        }
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
    fn from_intersection(
        db: &'db dyn Db,
        intersection: IntersectionType<'db>,
        active_types: &mut FxHashSet<Type<'db>>,
    ) -> Option<Self> {
        if let Some(complement) = intersection.enum_complement(db) {
            return Self::from_type(db, Type::EnumComplement(complement), active_types);
        }

        // Other intersection components can only reduce the represented enum values. Ignoring
        // them therefore preserves a safe upper bound without transferring them during narrowing.
        let mut value_sets = intersection
            .positive(db)
            .iter()
            .filter_map(|positive| Self::from_type(db, *positive, active_types));
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

    fn member_promotability(&self, db: &'db dyn Db, name: &Name) -> Option<bool> {
        match &self.members {
            EnumValueSetMembers::All => Some(true),
            EnumValueSetMembers::One {
                name: member,
                promotable,
            } => (*member == name).then_some(*promotable),
            EnumValueSetMembers::Included(members) => members.get(name).copied(),
            EnumValueSetMembers::AllExcept(complement) => {
                (!complement.excluded_names(db).contains(name)).then_some(true)
            }
        }
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
                        complement.excluded_names(db),
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
        LiteralValueType::new(EnumLiteralType::new(db, self.enum_class, name), promotable).into()
    }
}

/// One or more enum-class domains represented by an operand.
struct EnumDomainSet<'db> {
    domains: Vec<EnumValueSet<'db>>,
}

impl<'db> EnumDomainSet<'db> {
    fn from_type(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        fn collect<'db>(
            db: &'db dyn Db,
            ty: Type<'db>,
            domains: &mut Vec<EnumValueSet<'db>>,
            active_types: &mut FxHashSet<Type<'db>>,
        ) -> Option<()> {
            if let Some(domain) = EnumValueSet::from_type(db, ty, active_types) {
                domains.push(domain);
                return Some(());
            }

            if !active_types.insert(ty) {
                return None;
            }
            let result = collect_union(db, ty, domains, active_types);
            active_types.remove(&ty);
            result
        }

        fn collect_union<'db>(
            db: &'db dyn Db,
            ty: Type<'db>,
            domains: &mut Vec<EnumValueSet<'db>>,
            active_types: &mut FxHashSet<Type<'db>>,
        ) -> Option<()> {
            let Type::Union(union) = ty.resolve_type_alias(db) else {
                return None;
            };
            for element in union.elements(db) {
                collect(db, *element, domains, active_types)?;
            }
            Some(())
        }

        let mut domains = Vec::new();
        let mut active_types = FxHashSet::default();
        collect(db, ty, &mut domains, &mut active_types)?;
        (!domains.is_empty()).then_some(Self { domains })
    }

    fn single(&self) -> Option<&EnumValueSet<'db>> {
        let [domain] = self.domains.as_slice() else {
            return None;
        };
        Some(domain)
    }

    fn key_projection(
        &self,
        db: &'db dyn Db,
        operator: ComparisonOperator,
    ) -> Option<EnumKeyProjection<'db>> {
        let mut projection = EnumKeyProjection::default();
        for domain in &self.domains {
            domain.add_keys_to_projection(db, operator, &mut projection)?;
        }
        Some(projection)
    }

    fn restriction_type(&self, db: &'db dyn Db) -> Type<'db> {
        self.domains
            .iter()
            .fold(UnionBuilder::new(db), |builder, domain| {
                builder.add(domain.restriction_type(db))
            })
            .build()
    }

    fn restrict_for_equality(
        &self,
        db: &'db dyn Db,
        operator: ComparisonOperator,
        other: &EnumKeyProjection<'db>,
    ) -> Option<Type<'db>> {
        let mut builder = UnionBuilder::new(db);
        for domain in &self.domains {
            let mut projection = EnumKeyProjection::default();
            domain.add_keys_to_projection(db, operator, &mut projection)?;
            if projection.unknowns_may_overlap(other) {
                builder = builder.add(domain.restriction_type(db));
            } else if let Some(retained) = domain.retain_keys(db, operator, &other.keys).ok()? {
                builder = builder.add(retained.restriction_type(db));
            }
        }
        Some(builder.build())
    }

    /// Return the known subset of `self` that compares equal to `other`'s single key.
    fn known_equal_type(
        &self,
        db: &'db dyn Db,
        operator: ComparisonOperator,
        other: &EnumKeyProjection<'db>,
    ) -> Option<Type<'db>> {
        let mut builder = UnionBuilder::new(db);
        for domain in &self.domains {
            let mut projection = EnumKeyProjection::default();
            domain.add_keys_to_projection(db, operator, &mut projection)?;
            if projection.unknowns_may_overlap(other) {
                continue;
            }
            if let Some(retained) = domain.retain_keys(db, operator, &other.keys).ok()? {
                builder = builder.add(retained.restriction_type(db));
            }
        }
        Some(builder.build())
    }
}

/// A comparison of operands that span more than one enum class.
struct ProjectedEnumComparison<'db> {
    left: EnumDomainSet<'db>,
    left_projection: EnumKeyProjection<'db>,
    right_projection: EnumKeyProjection<'db>,
}

impl<'db> ProjectedEnumComparison<'db> {
    fn new(
        db: &'db dyn Db,
        left: EnumDomainSet<'db>,
        right: &EnumDomainSet<'db>,
        operator: ComparisonOperator,
    ) -> Option<Self> {
        let left_projection = left.key_projection(db, operator)?;
        let right_projection = right.key_projection(db, operator)?;
        Some(Self {
            left,
            left_projection,
            right_projection,
        })
    }

    fn truthiness(&self, operator: ComparisonOperator) -> Truthiness {
        let equality = if !self.left_projection.may_overlap(&self.right_projection) {
            Truthiness::AlwaysFalse
        } else if let (Some(left), Some(right)) = (
            self.left_projection.single_key(),
            self.right_projection.single_key(),
        ) && left == right
        {
            Truthiness::AlwaysTrue
        } else {
            Truthiness::Ambiguous
        };
        equality.negate_if(operator == ComparisonOperator::Inequality)
    }

    fn evaluate(
        &self,
        db: &'db dyn Db,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> Option<ComparisonResult<'db>> {
        match self.truthiness(operator) {
            Truthiness::AlwaysTrue => Some(ComparisonResult::AlwaysTrue),
            Truthiness::AlwaysFalse => Some(ComparisonResult::AlwaysFalse),
            Truthiness::Ambiguous if operator.condition_expects_equality(branch) => {
                Some(ComparisonResult::CanNarrow(
                    self.left
                        .restrict_for_equality(db, operator, &self.right_projection)?,
                ))
            }
            Truthiness::Ambiguous if self.right_projection.single_key().is_some() => {
                let equal_left =
                    self.left
                        .known_equal_type(db, operator, &self.right_projection)?;
                Some(ComparisonResult::CanNarrow(
                    IntersectionBuilder::new(db)
                        .add_positive(self.left.restriction_type(db))
                        .add_negative(equal_left)
                        .build(),
                ))
            }
            Truthiness::Ambiguous => Some(ComparisonResult::Ambiguous),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum EnumComparisonKey<'db> {
    Object(EnumClassLiteral<'db>, &'db Name),
    Scalar(LiteralValueTypeKind<'db>),
}

impl<'db> EnumComparisonKey<'db> {
    fn domain(self) -> Option<EnumComparisonKeyDomain<'db>> {
        match self {
            Self::Object(enum_class, _) => Some(EnumComparisonKeyDomain::Object(enum_class)),
            Self::Scalar(LiteralValueTypeKind::Int(_) | LiteralValueTypeKind::Bool(_)) => {
                Some(EnumComparisonKeyDomain::Int)
            }
            Self::Scalar(LiteralValueTypeKind::String(_)) => Some(EnumComparisonKeyDomain::Str),
            Self::Scalar(LiteralValueTypeKind::Bytes(_)) => Some(EnumComparisonKeyDomain::Bytes),
            Self::Scalar(_) => None,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum EnumComparisonKeyDomain<'db> {
    Object(EnumClassLiteral<'db>),
    Int,
    Str,
    Bytes,
    Tuple,
    Dict,
}

impl<'db> EnumComparisonKeyDomain<'db> {
    fn new(enum_class: EnumClassLiteral<'db>, semantics: KnownComparisonSemantics) -> Self {
        match semantics {
            KnownComparisonSemantics::Object => Self::Object(enum_class),
            KnownComparisonSemantics::Int => Self::Int,
            KnownComparisonSemantics::Str => Self::Str,
            KnownComparisonSemantics::Bytes => Self::Bytes,
            KnownComparisonSemantics::Tuple => Self::Tuple,
            KnownComparisonSemantics::Dict => Self::Dict,
        }
    }

    const fn unknown_overlaps_known(self) -> bool {
        !matches!(self, Self::Object(_))
    }
}

#[derive(Default)]
struct EnumKeyProjection<'db> {
    keys: FxHashSet<EnumComparisonKey<'db>>,
    known_domains: FxHashSet<EnumComparisonKeyDomain<'db>>,
    unknown_domains: FxHashSet<EnumComparisonKeyDomain<'db>>,
}

impl<'db> EnumKeyProjection<'db> {
    fn may_overlap(&self, other: &Self) -> bool {
        !self.keys.is_disjoint(&other.keys) || self.unknowns_may_overlap(other)
    }

    fn unknowns_may_overlap(&self, other: &Self) -> bool {
        !self.unknown_domains.is_disjoint(&other.unknown_domains)
            || self.unknown_domains.iter().any(|domain| {
                domain.unknown_overlaps_known() && other.known_domains.contains(domain)
            })
            || other.unknown_domains.iter().any(|domain| {
                domain.unknown_overlaps_known() && self.known_domains.contains(domain)
            })
    }

    fn single_key(&self) -> Option<EnumComparisonKey<'db>> {
        if self.unknown_domains.is_empty() && self.keys.len() == 1 {
            self.keys.iter().copied().next()
        } else {
            None
        }
    }
}

impl<'db> EnumValueSet<'db> {
    fn add_keys_to_projection(
        &self,
        db: &'db dyn Db,
        operator: ComparisonOperator,
        projection: &mut EnumKeyProjection<'db>,
    ) -> Option<()> {
        let profile = enum_class_key_profile(db, self.enum_class, operator);
        let semantics = profile.semantics?;
        let key_domain = EnumComparisonKeyDomain::new(self.enum_class, semantics);

        for (name, scalar_key) in &profile.members {
            if self.member_promotability(db, name).is_none() {
                continue;
            }
            let key = if semantics == KnownComparisonSemantics::Object {
                Some(EnumComparisonKey::Object(self.enum_class, name))
            } else {
                scalar_key.map(EnumComparisonKey::Scalar)
            };
            if let Some(key) = key
                && let Some(domain) = key.domain()
            {
                projection.known_domains.insert(domain);
                projection.keys.insert(key);
            } else {
                projection.unknown_domains.insert(key_domain);
            }
        }

        if !self.is_closed(profile.members_are_exhaustive) {
            projection.unknown_domains.insert(key_domain);
        }
        Some(())
    }

    fn retain_keys(
        &self,
        db: &'db dyn Db,
        operator: ComparisonOperator,
        keys: &FxHashSet<EnumComparisonKey<'db>>,
    ) -> Result<Option<Self>, ()> {
        let profile = enum_class_key_profile(db, self.enum_class, operator);
        let semantics = profile.semantics.ok_or(())?;
        let mut included = FxOrderMap::default();
        for (name, scalar_key) in &profile.members {
            let Some(promotable) = self.member_promotability(db, name) else {
                continue;
            };
            let key = if semantics == KnownComparisonSemantics::Object {
                Some(EnumComparisonKey::Object(self.enum_class, name))
            } else {
                scalar_key.map(EnumComparisonKey::Scalar)
            };
            if key.is_some_and(|key| keys.contains(&key)) {
                Self::insert_member(&mut included, name, promotable);
            }
        }
        if included.len() == self.member_count(db) && self.is_closed(profile.members_are_exhaustive)
        {
            return Ok(Some(self.clone()));
        }

        Ok(Self::from_included(self.enum_class, included))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
struct EnumClassKeyProfile<'db> {
    members_are_exhaustive: bool,
    semantics: Option<KnownComparisonSemantics>,
    members: Box<[(Name, Option<LiteralValueTypeKind<'db>>)]>,
}

/// Cache each class's modeled comparison keys independently of any particular operand pair.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
fn enum_class_key_profile<'db>(
    db: &'db dyn Db,
    enum_class: EnumClassLiteral<'db>,
    operator: ComparisonOperator,
) -> EnumClassKeyProfile<'db> {
    let semantics = KnownComparisonSemantics::of_instance(
        db,
        enum_class.class_literal(db).to_non_generic_instance(db),
        operator,
    );
    let members: Box<[(Name, Option<LiteralValueTypeKind<'db>>)]> = enum_class
        .members(db)
        .iter()
        .map(|(name, _)| {
            (
                name.clone(),
                semantics.and_then(|semantics| {
                    enum_literal_value(db, EnumLiteralType::new(db, enum_class, name))
                        .and_then(|value| enum_comparison_key(semantics, value))
                }),
            )
        })
        .collect();
    EnumClassKeyProfile {
        members_are_exhaustive: enum_class.members_are_exhaustive(db),
        semantics,
        members,
    }
}

/// Whether distinct declared members are known to have distinct runtime comparison keys.
#[derive(Debug, Copy, Clone, PartialEq, Eq, get_size2::GetSize)]
enum SameEnumComparisonKeys {
    /// Different member names cannot compare equal.
    Distinct,
    /// Values are unknown or repeated, so different member names may compare equal.
    UnknownOrRepeated,
}

/// Same-class facts that are unnecessary when projecting keys across different classes.
#[derive(Debug, Copy, Clone, PartialEq, Eq, get_size2::GetSize)]
struct SameEnumComparisonProfile {
    members_are_exhaustive: bool,
    members_compare_by_identity: bool,
    comparison_keys: Option<SameEnumComparisonKeys>,
}

#[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
fn same_enum_comparison_profile<'db>(
    db: &'db dyn Db,
    enum_class: EnumClassLiteral<'db>,
    operator: ComparisonOperator,
) -> SameEnumComparisonProfile {
    let profile = enum_class_key_profile(db, enum_class, operator);
    let (comparison_keys, members_compare_by_identity) = match profile.semantics {
        None => (None, false),
        Some(KnownComparisonSemantics::Object) if !enum_class.aliases_are_known(db) => {
            (Some(SameEnumComparisonKeys::UnknownOrRepeated), true)
        }
        Some(KnownComparisonSemantics::Object) => (Some(SameEnumComparisonKeys::Distinct), true),
        Some(
            KnownComparisonSemantics::Int
            | KnownComparisonSemantics::Str
            | KnownComparisonSemantics::Bytes,
        ) => {
            let mut keys = FxHashSet::default();
            let keys_are_distinct = profile
                .members
                .iter()
                .all(|(_, key)| key.is_some_and(|key| keys.insert(key)));
            (
                Some(if keys_are_distinct {
                    SameEnumComparisonKeys::Distinct
                } else {
                    SameEnumComparisonKeys::UnknownOrRepeated
                }),
                false,
            )
        }
        Some(_) => (Some(SameEnumComparisonKeys::UnknownOrRepeated), false),
    };
    SameEnumComparisonProfile {
        members_are_exhaustive: profile.members_are_exhaustive,
        members_compare_by_identity,
        comparison_keys,
    }
}

fn enum_comparison_key(
    semantics: KnownComparisonSemantics,
    value: Type<'_>,
) -> Option<LiteralValueTypeKind<'_>> {
    match (semantics, value.as_literal_value_kind()) {
        (KnownComparisonSemantics::Int, Some(LiteralValueTypeKind::Bool(value))) => Some(
            LiteralValueTypeKind::Int(IntLiteralType::from_i64(i64::from(value))),
        ),
        (KnownComparisonSemantics::Int, Some(kind @ LiteralValueTypeKind::Int(_)))
        | (KnownComparisonSemantics::Str, Some(kind @ LiteralValueTypeKind::String(_)))
        | (KnownComparisonSemantics::Bytes, Some(kind @ LiteralValueTypeKind::Bytes(_))) => {
            Some(kind)
        }
        _ => None,
    }
}
