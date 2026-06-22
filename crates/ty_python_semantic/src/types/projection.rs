use std::cell::RefCell;

use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use rustc_hash::{FxHashMap, FxHashSet};
use ty_python_core::EvaluationMode;

use crate::place::{DefinedPlace, Definedness, Place};
use crate::subscript::{PyIndex, PySlice};
use crate::types::tuple::{TupleLength, TupleSpec, VariableLengthTuple};
use crate::types::visitor::{TypeCollector, TypeVisitor, walk_type_with_recursion_guard};
use crate::types::{
    ApplyTypeMappingVisitor, CallArguments, DivergentType, KnownClass, MemberLookupPolicy, Type,
    TypeContext, TypeMapping, UnionType,
};
use crate::{Db, FxOrderSet};

/// An operation applied to a recursive cycle marker before the cycle has been recovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub enum ProjectionOp<'db> {
    Subscript {
        slice: Type<'db>,
        expr_context: ast::ExprContext,
    },
    IterItem {
        is_async: bool,
    },
    UnpackExact {
        len: u32,
        index: u32,
    },
    UnpackStarred {
        before: u32,
        after: u32,
        target: ProjectionUnpackTarget,
    },
    Binary {
        op: ast::Operator,
        other: Type<'db>,
        is_reverse: bool,
    },
    Member(ProjectionMember<'db>),
    CallMethod0(ProjectionMemberName<'db>),
    ContextEnter {
        is_async: bool,
    },
    AwaitResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub enum ProjectionUnpackTarget {
    Prefix { index: u32 },
    Starred,
    Suffix { index: u32 },
}

/// An interned member name used by attribute and method-call projections.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct ProjectionMemberName<'db> {
    #[returns(ref)]
    name: Name,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ProjectionMemberName<'_> {}

impl<'db> ProjectionMemberName<'db> {
    fn new(db: &'db dyn Db, name: &Name) -> Self {
        let mut name = name.clone();
        name.shrink_to_fit();
        Self::new_internal(db, name)
    }
}

/// An attribute lookup projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub struct ProjectionMember<'db> {
    name: ProjectionMemberName<'db>,
    policy: ProjectionMemberLookupPolicy,
}

impl<'db> ProjectionMember<'db> {
    fn new(db: &'db dyn Db, name: &Name, policy: MemberLookupPolicy) -> Self {
        Self {
            name: ProjectionMemberName::new(db, name),
            policy: ProjectionMemberLookupPolicy::new(policy),
        }
    }

    fn name(self, db: &'db dyn Db) -> &'db Name {
        self.name.name(db)
    }

    fn policy(self) -> MemberLookupPolicy {
        self.policy.to_policy()
    }
}

/// Compact copyable member lookup policy stored in projection paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
struct ProjectionMemberLookupPolicy(u8);

impl ProjectionMemberLookupPolicy {
    const fn new(policy: MemberLookupPolicy) -> Self {
        Self(policy.bits())
    }

    fn to_policy(self) -> MemberLookupPolicy {
        MemberLookupPolicy::from_bits_retain(self.0)
    }
}

impl<'db> ProjectionOp<'db> {
    pub(crate) fn member(db: &'db dyn Db, name: &Name, policy: MemberLookupPolicy) -> Self {
        Self::Member(ProjectionMember::new(db, name, policy))
    }

    pub(crate) fn call_method0(db: &'db dyn Db, name: &Name) -> Self {
        Self::CallMethod0(ProjectionMemberName::new(db, name))
    }

    fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        if matches!(type_mapping, TypeMapping::Promote(..)) {
            return self;
        }

        match self {
            Self::Subscript {
                slice,
                expr_context,
            } => Self::Subscript {
                slice: slice.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                expr_context,
            },
            Self::Binary {
                op,
                other,
                is_reverse,
            } => Self::Binary {
                op,
                other: other.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                is_reverse,
            },
            Self::IterItem { .. }
            | Self::UnpackExact { .. }
            | Self::UnpackStarred { .. }
            | Self::Member(_)
            | Self::CallMethod0(_)
            | Self::ContextEnter { .. }
            | Self::AwaitResult => self,
        }
    }
}

/// The result of applying a projection path to an active recursive cycle root.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct ProjectionType<'db> {
    pub(crate) root: DivergentType,

    #[returns(deref)]
    pub(crate) path: Box<[ProjectionOp<'db>]>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ProjectionType<'_> {}

impl<'db> ProjectionType<'db> {
    pub(crate) fn append(self, db: &'db dyn Db, op: ProjectionOp<'db>) -> Self {
        let mut path = self.path(db).to_vec();
        path.push(op);
        Self::new(db, self.root(db), path.into_boxed_slice())
    }

    fn try_apply_path(self, db: &'db dyn Db, mut ty: Type<'db>) -> Option<Type<'db>> {
        for op in self.path(db) {
            ty = try_apply_projection_op(db, ty, *op)?;
        }
        Some(ty)
    }

    pub(crate) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        let mut changed = false;
        let path = self
            .path(db)
            .iter()
            .map(|op| {
                let mapped = op.apply_type_mapping_impl(db, type_mapping, tcx, visitor);
                changed |= mapped != *op;
                mapped
            })
            .collect::<Box<_>>();

        if changed {
            Self::new(db, self.root(db), path)
        } else {
            self
        }
    }
}

pub(crate) fn walk_projection_type<'db, V: TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    projection: ProjectionType<'db>,
    visitor: &V,
) {
    for op in projection.path(db) {
        match *op {
            ProjectionOp::Subscript { slice, .. } => visitor.visit_type(db, slice),
            ProjectionOp::Binary { other, .. } => visitor.visit_type(db, other),
            ProjectionOp::IterItem { .. }
            | ProjectionOp::UnpackExact { .. }
            | ProjectionOp::UnpackStarred { .. }
            | ProjectionOp::Member(_)
            | ProjectionOp::CallMethod0(_)
            | ProjectionOp::ContextEnter { .. }
            | ProjectionOp::AwaitResult => {}
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ProjectionRecoverySlot<'db> {
    previous: Option<Type<'db>>,
    joined: Type<'db>,
    role: ProjectionRecoverySlotRole,
}

#[derive(Debug, Clone, Copy)]
enum ProjectionRecoverySlotRole {
    /// A derived value that may demand projection solutions but must not define a root equation.
    DemandOnly,
    /// A query-managed value whose fixed-point slot can define a root equation.
    Candidate,
}

impl<'db> ProjectionRecoverySlot<'db> {
    pub(crate) const fn demand(previous: Option<Type<'db>>, joined: Type<'db>) -> Self {
        Self {
            previous,
            joined,
            role: ProjectionRecoverySlotRole::DemandOnly,
        }
    }

    pub(crate) const fn candidate(previous: Option<Type<'db>>, joined: Type<'db>) -> Self {
        Self {
            previous,
            joined,
            role: ProjectionRecoverySlotRole::Candidate,
        }
    }
}

#[derive(Debug)]
struct Equation<'db> {
    rhs: Type<'db>,
    dependencies: FxOrderSet<ProjectionType<'db>>,
}

pub(crate) fn solve_projections_in_cycle_slots<'db>(
    db: &'db dyn Db,
    slots: &[ProjectionRecoverySlot<'db>],
    cycle: &salsa::Cycle,
) -> Vec<Type<'db>> {
    let recovery_roots = active_roots(cycle);
    solve_projections_in_slots(db, slots, &recovery_roots, Some(cycle))
}

fn solve_projections_in_slots<'db>(
    db: &'db dyn Db,
    slots: &[ProjectionRecoverySlot<'db>],
    recovery_roots: &[DivergentType],
    cycle: Option<&salsa::Cycle>,
) -> Vec<Type<'db>> {
    if recovery_roots.is_empty() {
        return slots.iter().map(|slot| slot.joined).collect();
    }

    let candidates = root_candidates(db, slots, recovery_roots);
    let projection_results = projection_results_from_slots(db, slots, recovery_roots, &candidates);
    let collected = collect_slot_projections(db, slots, recovery_roots);
    let pending = collected
        .into_iter()
        .filter(|projection| {
            candidates.contains_key(&projection.root(db))
                || projection_results.contains_key(projection)
        })
        .collect::<FxOrderSet<_>>();
    let mut pending = remove_prefixed_projections(db, &pending);
    if pending.is_empty() {
        return slots
            .iter()
            .map(|slot| {
                if collect_active_projections(db, slot.joined, &[]).is_empty() {
                    normalize_recovered_type(db, slot.joined, cycle)
                } else {
                    normalize_recovered_type(
                        db,
                        replace_projections_with_roots(db, slot.joined),
                        cycle,
                    )
                }
            })
            .collect();
    }

    let mut equations = FxHashMap::default();
    let mut index = 0;
    while let Some(&projection) = pending.get_index(index) {
        index += 1;
        if equations.contains_key(&projection) {
            continue;
        }

        let mut rhs_terms = Vec::with_capacity(2);
        if let Some(result) = projection_results.get(&projection).copied() {
            rhs_terms.push(result);
        }
        if let Some(candidate) = candidates.get(&projection.root(db))
            && let Some(projected) = projection.try_apply_path(db, *candidate)
        {
            rhs_terms.push(projected);
        }
        let rhs = match rhs_terms.as_slice() {
            [] => Type::Divergent(projection.root(db)),
            [rhs] => *rhs,
            _ => UnionType::from_elements_cycle_recovery(db, rhs_terms),
        };
        let dependencies = collect_active_projections(db, rhs, recovery_roots);
        pending.extend(dependencies.iter().copied().filter(|dependency| {
            candidates.contains_key(&dependency.root(db))
                || projection_results.contains_key(dependency)
        }));
        pending = remove_prefixed_projections(db, &pending);
        equations.insert(projection, Equation { rhs, dependencies });
    }

    let solutions = solve_equations(db, &equations);
    slots
        .iter()
        .map(|slot| {
            if collect_active_projections(db, slot.joined, &[]).is_empty() {
                return normalize_recovered_type(db, slot.joined, cycle);
            }

            let recovered = slot.joined.replace_projections(db, &solutions);
            let recovered = replace_projections_with_roots(db, recovered);
            normalize_recovered_type(db, recovered, cycle)
        })
        .collect()
}

fn remove_prefixed_projections<'db>(
    db: &'db dyn Db,
    projections: &FxOrderSet<ProjectionType<'db>>,
) -> FxOrderSet<ProjectionType<'db>> {
    projections
        .iter()
        .copied()
        .filter(|projection| {
            !projections.iter().copied().any(|candidate| {
                projection.root(db).same_marker(candidate.root(db))
                    && candidate.path(db).len() < projection.path(db).len()
                    && projection.path(db).starts_with(candidate.path(db))
            })
        })
        .collect()
}

fn normalize_recovered_type<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    cycle: Option<&salsa::Cycle>,
) -> Type<'db> {
    if let Some(cycle) = cycle {
        ty.recursive_type_normalized(db, cycle)
    } else {
        ty
    }
}

fn active_roots(cycle: &salsa::Cycle) -> Vec<DivergentType> {
    cycle
        .head_ids()
        .map(|id| Type::divergent(id).as_divergent())
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default()
}

fn root_candidates<'db>(
    db: &'db dyn Db,
    slots: &[ProjectionRecoverySlot<'db>],
    active_roots: &[DivergentType],
) -> FxHashMap<DivergentType, Type<'db>> {
    let mut candidates = FxHashMap::default();
    for slot in slots {
        if !matches!(slot.role, ProjectionRecoverySlotRole::Candidate) {
            continue;
        }
        let root = if let Some(previous) = slot.previous {
            direct_active_root(db, previous, active_roots).or_else(|| {
                collect_active_roots(db, previous, active_roots)
                    .is_empty()
                    .then(|| unique_active_root(db, slot.joined, active_roots))
                    .flatten()
            })
        } else {
            unique_active_root(db, slot.joined, active_roots)
        };
        let Some(root) = root else {
            continue;
        };
        if !is_plausible_root_candidate(db, root, slot.joined) {
            continue;
        }
        candidates
            .entry(root)
            .and_modify(|candidate| {
                *candidate = UnionType::from_elements_cycle_recovery(db, [*candidate, slot.joined]);
            })
            .or_insert(slot.joined);
    }
    candidates
}

fn collect_slot_projections<'db>(
    db: &'db dyn Db,
    slots: &[ProjectionRecoverySlot<'db>],
    active_roots: &[DivergentType],
) -> FxOrderSet<ProjectionType<'db>> {
    let mut projections = FxOrderSet::default();
    for slot in slots {
        projections.extend(collect_active_projections(db, slot.joined, active_roots));
    }
    projections
}

fn projection_results_from_slots<'db>(
    db: &'db dyn Db,
    slots: &[ProjectionRecoverySlot<'db>],
    active_roots: &[DivergentType],
    candidates: &FxHashMap<DivergentType, Type<'db>>,
) -> FxHashMap<ProjectionType<'db>, Type<'db>> {
    let mut results = FxHashMap::<ProjectionType<'db>, Type<'db>>::default();
    for slot in slots {
        if matches!(slot.role, ProjectionRecoverySlotRole::Candidate) {
            for projection in top_level_active_projections(db, slot.joined, active_roots) {
                push_projection_result(db, &mut results, projection, slot.joined);
            }
        }
        for projection in collect_active_projections(db, slot.joined, active_roots) {
            if candidates.contains_key(&projection.root(db)) {
                continue;
            }
            if let Some(result) = structural_projection_result(db, slot.joined, projection) {
                push_projection_result(db, &mut results, projection, result);
            }
        }
    }
    results
}

fn push_projection_result<'db>(
    db: &'db dyn Db,
    results: &mut FxHashMap<ProjectionType<'db>, Type<'db>>,
    projection: ProjectionType<'db>,
    ty: Type<'db>,
) {
    results
        .entry(projection)
        .and_modify(|result| {
            *result = UnionType::from_elements_cycle_recovery(db, [*result, ty]);
        })
        .or_insert(ty);
}

fn structural_projection_result<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    projection: ProjectionType<'db>,
) -> Option<Type<'db>> {
    let mut matches = Vec::new();
    collect_structural_projection_result(db, ty, projection, &mut matches);

    (!matches.is_empty()).then(|| UnionType::from_elements_cycle_recovery(db, matches))
}

fn collect_structural_projection_result<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    projection: ProjectionType<'db>,
    matches: &mut Vec<Type<'db>>,
) {
    if let Type::Union(union) = ty {
        for pattern in union
            .elements(db)
            .iter()
            .copied()
            .filter(|element| element.contains_projection(db, projection))
        {
            for candidate in union
                .elements(db)
                .iter()
                .copied()
                .filter(|candidate| !candidate.contains_projection(db, projection))
            {
                collect_structural_projection_matches(
                    db, pattern, candidate, projection, matches, false,
                );
            }
        }

        for element in union.elements(db) {
            collect_structural_projection_result(db, *element, projection, matches);
        }
        return;
    }

    if let Some(tuple) = ty.exact_tuple_instance_spec(db) {
        for element in tuple.as_ref().all_elements() {
            collect_structural_projection_result(db, *element, projection, matches);
        }
    }

    if let Some((_, specialization)) = ty.direct_class_specialization(db) {
        for argument in specialization.types(db) {
            collect_structural_projection_result(db, *argument, projection, matches);
        }
    }
}

fn collect_structural_projection_matches<'db>(
    db: &'db dyn Db,
    pattern: Type<'db>,
    candidate: Type<'db>,
    projection: ProjectionType<'db>,
    matches: &mut Vec<Type<'db>>,
    nested: bool,
) {
    if pattern == Type::Projection(projection) {
        if nested {
            matches.push(candidate);
        }
        return;
    }

    if let (Some(pattern_tuple), Some(candidate_tuple)) = (
        pattern.exact_tuple_instance_spec(db),
        candidate.exact_tuple_instance_spec(db),
    ) {
        let pattern_elements = pattern_tuple.as_ref().all_elements();
        let candidate_elements = candidate_tuple.as_ref().all_elements();
        if pattern_elements.len() == candidate_elements.len() {
            for (&pattern, &candidate) in pattern_elements.iter().zip(candidate_elements) {
                collect_structural_projection_matches(
                    db, pattern, candidate, projection, matches, true,
                );
            }
        }
    }

    let (
        Some((pattern_class, pattern_specialization)),
        Some((candidate_class, candidate_specialization)),
    ) = (
        pattern.direct_class_specialization(db),
        candidate.direct_class_specialization(db),
    )
    else {
        return;
    };

    if pattern_class != candidate_class {
        return;
    }

    let pattern_arguments = pattern_specialization.types(db);
    let candidate_arguments = candidate_specialization.types(db);
    if pattern_arguments.len() != candidate_arguments.len() {
        return;
    }

    for (&pattern, &candidate) in pattern_arguments.iter().zip(candidate_arguments) {
        collect_structural_projection_matches(db, pattern, candidate, projection, matches, true);
    }
}

fn top_level_active_projections<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    active_roots: &[DivergentType],
) -> FxOrderSet<ProjectionType<'db>> {
    let mut projections = FxOrderSet::default();
    match ty {
        Type::Projection(projection) if root_is_active(projection.root(db), active_roots) => {
            projections.insert(projection);
        }
        Type::Union(union) => {
            projections.extend(union.elements(db).iter().filter_map(|element| {
                let Type::Projection(projection) = *element else {
                    return None;
                };
                root_is_active(projection.root(db), active_roots).then_some(projection)
            }));
        }
        _ => {}
    }
    projections
}

fn collect_active_projections<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    active_roots: &[DivergentType],
) -> FxOrderSet<ProjectionType<'db>> {
    let collector = ProjectionCollector {
        active_roots,
        projections: RefCell::default(),
        recursion_guard: TypeCollector::default(),
    };
    collector.visit_type(db, ty);
    collector.projections.into_inner()
}

pub(super) fn replace_projections_with_roots<'db>(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
    let replacements = collect_active_projections(db, ty, &[])
        .into_iter()
        .map(|projection| (projection, Type::Divergent(projection.root(db))))
        .collect::<FxHashMap<_, _>>();
    ty.replace_projections(db, &replacements)
}

fn unique_active_root<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    active_roots: &[DivergentType],
) -> Option<DivergentType> {
    let mut unique = None;
    for root in active_roots {
        if ty.contains_divergent_marker(db, *root) {
            if unique.is_some() {
                return None;
            }
            unique = Some(*root);
        }
    }
    unique
}

fn collect_active_roots<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    active_roots: &[DivergentType],
) -> Vec<DivergentType> {
    active_roots
        .iter()
        .copied()
        .filter(|root| ty.contains_divergent_marker(db, *root))
        .collect()
}

fn direct_active_root<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    active_roots: &[DivergentType],
) -> Option<DivergentType> {
    let root = match ty {
        Type::Divergent(root) => root,
        Type::Projection(projection) => projection.root(db),
        _ => return None,
    };
    root_is_active(root, active_roots).then_some(root)
}

fn is_plausible_root_candidate<'db>(db: &'db dyn Db, root: DivergentType, ty: Type<'db>) -> bool {
    union_elements(db, ty).iter().copied().any(|element| {
        !is_same_cycle_artifact(db, element, root) && is_projection_container_candidate(db, element)
    })
}

fn is_same_cycle_artifact<'db>(db: &'db dyn Db, ty: Type<'db>, root: DivergentType) -> bool {
    match ty {
        Type::Divergent(divergent) => divergent.same_marker(root),
        Type::Projection(projection) => projection.root(db).same_marker(root),
        _ => false,
    }
}

fn is_projection_container_candidate<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
    if ty.exact_tuple_instance_spec(db).is_some() {
        return true;
    }

    ty.direct_class_specialization(db)
        .is_some_and(|(_, specialization)| !specialization.types(db).is_empty())
}

struct ProjectionCollector<'db, 'a> {
    active_roots: &'a [DivergentType],
    projections: RefCell<FxOrderSet<ProjectionType<'db>>>,
    recursion_guard: TypeCollector<'db>,
}

impl<'db> TypeVisitor<'db> for ProjectionCollector<'db, '_> {
    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        if let Type::Projection(projection) = ty
            && (self.active_roots.is_empty()
                || root_is_active(projection.root(db), self.active_roots))
        {
            self.projections.borrow_mut().insert(projection);
        }

        walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
    }
}

fn root_is_active(root: DivergentType, active_roots: &[DivergentType]) -> bool {
    active_roots
        .iter()
        .any(|active_root| active_root.same_marker(root))
}

fn try_apply_projection_op<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    op: ProjectionOp<'db>,
) -> Option<Type<'db>> {
    if let Some(projected) = ty.project_cycle(db, op) {
        return Some(projected);
    }

    if let Type::Union(union) = ty {
        let elements = union
            .elements(db)
            .iter()
            .filter_map(|element| try_apply_projection_op(db, *element, op))
            .collect::<Vec<_>>();
        return (!elements.is_empty())
            .then(|| UnionType::from_elements_cycle_recovery(db, elements));
    }

    if let Some(projected) = apply_projection_op_structurally(db, ty, op) {
        return Some(projected);
    }

    Some(match op {
        ProjectionOp::Subscript {
            slice,
            expr_context,
        } => ty.subscript(db, slice, expr_context).ok()?,
        ProjectionOp::IterItem { is_async } => {
            let mode = if is_async {
                EvaluationMode::Async
            } else {
                EvaluationMode::Sync
            };
            ty.try_iterate_with_mode(db, mode)
                .ok()?
                .homogeneous_element_type(db)
        }
        ProjectionOp::UnpackExact { len, index } => {
            try_apply_unpack_exact_projection(db, ty, len, index)?
        }
        ProjectionOp::UnpackStarred {
            before,
            after,
            target,
        } => try_apply_unpack_starred_projection(db, ty, before, after, target)?,
        ProjectionOp::Binary {
            op,
            other,
            is_reverse,
        } => {
            let (left, right) = if is_reverse { (other, ty) } else { (ty, other) };
            Type::try_call_bin_op_return_type(db, left, op, right)?
        }
        ProjectionOp::Member(member) => {
            infer_member_type_for_type(db, ty, member.name(db), member.policy())?
        }
        ProjectionOp::CallMethod0(method_name) => {
            infer_method_call0_type_for_type(db, ty, method_name.name(db))?
        }
        ProjectionOp::ContextEnter { is_async } => {
            let mode = EvaluationMode::from_is_async(is_async);
            ty.try_enter_with_mode(db, mode).ok()?
        }
        ProjectionOp::AwaitResult => ty.try_await(db).ok()?,
    })
}

fn apply_projection_op_structurally<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    op: ProjectionOp<'db>,
) -> Option<Type<'db>> {
    if let Type::Union(union) = ty {
        let elements = union
            .elements(db)
            .iter()
            .map(|element| apply_projection_op_structurally(db, *element, op))
            .collect::<Option<Vec<_>>>()?;
        return Some(UnionType::from_elements_cycle_recovery(db, elements));
    }

    match op {
        ProjectionOp::Subscript { slice, .. } => {
            apply_subscript_projection_structurally(db, ty, slice)
        }
        ProjectionOp::IterItem { is_async } => apply_iter_projection_structurally(db, ty, is_async),
        ProjectionOp::UnpackExact { len, index } => {
            apply_unpack_exact_projection_structurally(db, ty, len, index)
        }
        ProjectionOp::UnpackStarred {
            before,
            after,
            target,
        } => apply_unpack_starred_projection_structurally(db, ty, before, after, target),
        ProjectionOp::CallMethod0(method_name) => {
            apply_zero_arg_method_projection_structurally(db, ty, method_name.name(db))
        }
        ProjectionOp::Binary { .. }
        | ProjectionOp::Member(_)
        | ProjectionOp::ContextEnter { .. }
        | ProjectionOp::AwaitResult => None,
    }
}

fn apply_iter_projection_structurally<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    is_async: bool,
) -> Option<Type<'db>> {
    if let Some(spec) = ty.exact_tuple_instance_spec(db) {
        return (!is_async).then(|| spec.as_ref().homogeneous_element_type(db));
    }

    let (known_class, arguments) = direct_known_class_arguments(db, ty)?;
    match (known_class, arguments, is_async) {
        (
            KnownClass::List
            | KnownClass::Set
            | KnownClass::FrozenSet
            | KnownClass::Deque
            | KnownClass::Iterable
            | KnownClass::Iterator
            | KnownClass::Sequence
            | KnownClass::TyExtensionsIterable
            | KnownClass::TyExtensionsIterator,
            [element],
            false,
        )
        | (
            KnownClass::AsyncIterator
            | KnownClass::TyExtensionsAsyncIterable
            | KnownClass::TyExtensionsAsyncIterator,
            [element],
            true,
        ) => Some(*element),
        (
            KnownClass::Dict
            | KnownClass::DefaultDict
            | KnownClass::OrderedDict
            | KnownClass::Mapping,
            [key, _],
            false,
        ) => Some(*key),
        _ => None,
    }
}

fn apply_subscript_projection_structurally<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    slice: Type<'db>,
) -> Option<Type<'db>> {
    if let Some(spec) = ty.exact_tuple_instance_spec(db) {
        let tuple = spec.as_ref();

        if let Some(index) = slice.as_int_like_literal() {
            return i32::try_from(index)
                .ok()
                .and_then(|index| tuple.py_index(db, index).ok());
        }

        if let Some(slice) = slice
            .as_nominal_instance()
            .and_then(|instance| instance.slice_literal(db))
        {
            return match tuple {
                TupleSpec::Fixed(tuple) => Some(Type::heterogeneous_tuple(
                    db,
                    tuple
                        .py_slice(db, slice.start, slice.stop, slice.step)
                        .ok()?,
                )),
                TupleSpec::Variable(tuple) => {
                    let element = UnionType::from_elements_leave_aliases(
                        db,
                        tuple
                            .iter_prefix_elements()
                            .chain(std::iter::once(tuple.variable()))
                            .chain(tuple.iter_suffix_elements()),
                    );
                    Some(Type::homogeneous_tuple(db, element))
                }
            };
        }
    }

    let (known_class, arguments) = direct_known_class_arguments(db, ty)?;
    match (known_class, arguments) {
        (KnownClass::List | KnownClass::Deque, [element]) if is_structural_int_index(db, slice) => {
            Some(*element)
        }
        (KnownClass::List | KnownClass::Deque, [element])
            if slice
                .as_nominal_instance()
                .and_then(|instance| instance.slice_literal(db))
                .is_some() =>
        {
            Some(KnownClass::List.to_specialized_instance(db, &[*element]))
        }
        (
            KnownClass::Dict
            | KnownClass::DefaultDict
            | KnownClass::OrderedDict
            | KnownClass::Mapping,
            [_, value],
        ) => Some(*value),
        _ => None,
    }
}

fn apply_unpack_exact_projection_structurally<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    len: u32,
    index: u32,
) -> Option<Type<'db>> {
    let len = usize::try_from(len).ok()?;
    let index = usize::try_from(index).ok()?;

    if let Some(spec) = ty.exact_tuple_instance_spec(db) {
        let tuple = spec.as_ref().resize(db, TupleLength::Fixed(len)).ok()?;
        return Some(
            tuple
                .all_elements()
                .get(index)
                .copied()
                .unwrap_or_else(Type::unknown),
        );
    }

    apply_iter_projection_structurally(db, ty, false)
}

fn apply_unpack_starred_projection_structurally<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    before: u32,
    after: u32,
    target: ProjectionUnpackTarget,
) -> Option<Type<'db>> {
    let before = usize::try_from(before).ok()?;
    let after = usize::try_from(after).ok()?;

    if let Some(spec) = ty.exact_tuple_instance_spec(db) {
        let TupleSpec::Variable(tuple) = spec
            .as_ref()
            .resize(db, TupleLength::Variable(before, after))
            .ok()?
        else {
            return None;
        };
        return Some(match target {
            ProjectionUnpackTarget::Prefix { index } => usize::try_from(index)
                .ok()
                .and_then(|index| tuple.prefix_elements().get(index).copied())
                .unwrap_or_else(Type::unknown),
            ProjectionUnpackTarget::Starred => tuple.variable(),
            ProjectionUnpackTarget::Suffix { index } => usize::try_from(index)
                .ok()
                .and_then(|index| tuple.suffix_elements().get(index).copied())
                .unwrap_or_else(Type::unknown),
        });
    }

    apply_iter_projection_structurally(db, ty, false)
}

fn apply_zero_arg_method_projection_structurally<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    method_name: &Name,
) -> Option<Type<'db>> {
    let (known_class, arguments) = direct_known_class_arguments(db, ty)?;
    let [key, value] = arguments else {
        return None;
    };
    if !matches!(
        known_class,
        KnownClass::Dict | KnownClass::DefaultDict | KnownClass::OrderedDict | KnownClass::Mapping
    ) {
        return None;
    }

    match method_name.as_str() {
        "keys" => Some(KnownClass::List.to_specialized_instance(db, &[*key])),
        "values" => Some(KnownClass::List.to_specialized_instance(db, &[*value])),
        "items" => {
            let item = Type::heterogeneous_tuple(db, [*key, *value]);
            Some(KnownClass::List.to_specialized_instance(db, &[item]))
        }
        _ => None,
    }
}

fn direct_known_class_arguments<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
) -> Option<(KnownClass, &'db [Type<'db>])> {
    let (class, specialization) = ty.direct_class_specialization(db)?;
    Some((class.known(db)?, specialization.types(db)))
}

fn is_structural_int_index(db: &dyn Db, ty: Type<'_>) -> bool {
    ty.as_int_like_literal().is_some()
        || matches!(
            ty.direct_known_class(db),
            Some(KnownClass::Int | KnownClass::Bool)
        )
}

fn infer_member_type_for_type<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    name: &Name,
    policy: MemberLookupPolicy,
) -> Option<Type<'db>> {
    let Place::Defined(DefinedPlace {
        ty,
        definedness: Definedness::AlwaysDefined,
        ..
    }) = ty.member_lookup_with_policy(db, name.clone(), policy).place
    else {
        return None;
    };

    Some(ty)
}

fn infer_method_call0_type_for_type<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    method_name: &Name,
) -> Option<Type<'db>> {
    let Place::Defined(DefinedPlace {
        ty: method,
        definedness: Definedness::AlwaysDefined,
        ..
    }) = ty
        .member_lookup_with_policy(
            db,
            method_name.clone(),
            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
        )
        .place
    else {
        return None;
    };

    Some(
        method
            .try_call(db, &CallArguments::none())
            .ok()?
            .return_type(db),
    )
}

fn try_apply_unpack_exact_projection<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    len: u32,
    index: u32,
) -> Option<Type<'db>> {
    if let Type::Union(union) = ty {
        let elements = union
            .elements(db)
            .iter()
            .filter_map(|element| try_apply_unpack_exact_projection(db, *element, len, index))
            .collect::<Vec<_>>();
        return (!elements.is_empty())
            .then(|| UnionType::from_elements_cycle_recovery(db, elements));
    }

    let index = usize::try_from(index).ok()?;
    let len = usize::try_from(len).ok()?;

    let tuple = ty.try_iterate(db).ok()?;
    let Some(fixed) = tuple.as_fixed_length() else {
        return Some(tuple.homogeneous_element_type(db));
    };
    if fixed.len() != len {
        return Some(tuple.homogeneous_element_type(db));
    }
    fixed.all_elements().get(index).copied()
}

fn try_apply_unpack_starred_projection<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    before: u32,
    after: u32,
    target: ProjectionUnpackTarget,
) -> Option<Type<'db>> {
    if let Type::Union(union) = ty {
        let elements = union
            .elements(db)
            .iter()
            .filter_map(|element| {
                try_apply_unpack_starred_projection(db, *element, before, after, target)
            })
            .collect::<Vec<_>>();
        return (!elements.is_empty())
            .then(|| UnionType::from_elements_cycle_recovery(db, elements));
    }

    let before = usize::try_from(before).ok()?;
    let after = usize::try_from(after).ok()?;

    let tuple = ty.try_iterate(db).ok()?;
    let Ok(TupleSpec::Variable(tuple)) = tuple.resize(db, TupleLength::Variable(before, after))
    else {
        return None;
    };

    match target {
        ProjectionUnpackTarget::Prefix { index } => usize::try_from(index)
            .ok()
            .and_then(|index| tuple.prefix_elements().get(index).copied()),
        ProjectionUnpackTarget::Starred => Some(tuple.variable()),
        ProjectionUnpackTarget::Suffix { index } => usize::try_from(index)
            .ok()
            .and_then(|index| tuple.suffix_elements().get(index).copied()),
    }
}

pub(crate) fn exact_unpack_projection_tuple<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    len: usize,
) -> Option<TupleSpec<'db>> {
    let len = u32::try_from(len).ok()?;
    ty.project_cycle(db, ProjectionOp::UnpackExact { len, index: 0 })?;

    Some(TupleSpec::heterogeneous((0..len).map(|index| {
        ty.project_cycle(db, ProjectionOp::UnpackExact { len, index })
            .unwrap_or_else(Type::unknown)
    })))
}

pub(crate) fn unpack_projection_tuple<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    target_len: TupleLength,
) -> Option<TupleSpec<'db>> {
    match target_len {
        TupleLength::Fixed(len) => exact_unpack_projection_tuple(db, ty, len),
        TupleLength::Variable(before, after) => {
            let before = u32::try_from(before).ok()?;
            let after = u32::try_from(after).ok()?;
            let variable = ty.project_cycle(
                db,
                ProjectionOp::UnpackStarred {
                    before,
                    after,
                    target: ProjectionUnpackTarget::Starred,
                },
            )?;

            Some(VariableLengthTuple::mixed(
                (0..before).map(|index| {
                    ty.project_cycle(
                        db,
                        ProjectionOp::UnpackStarred {
                            before,
                            after,
                            target: ProjectionUnpackTarget::Prefix { index },
                        },
                    )
                    .unwrap_or_else(Type::unknown)
                }),
                variable,
                (0..after).map(|index| {
                    ty.project_cycle(
                        db,
                        ProjectionOp::UnpackStarred {
                            before,
                            after,
                            target: ProjectionUnpackTarget::Suffix { index },
                        },
                    )
                    .unwrap_or_else(Type::unknown)
                }),
            ))
        }
    }
}

fn solve_equations<'db>(
    db: &'db dyn Db,
    equations: &FxHashMap<ProjectionType<'db>, Equation<'db>>,
) -> FxHashMap<ProjectionType<'db>, Type<'db>> {
    let components = Tarjan::new(equations).components();
    debug_assert!(components_are_dependency_first(&components, equations));

    let mut solutions = FxHashMap::default();
    for component in components {
        solve_component(db, equations, &component, &mut solutions);
    }
    solutions
}

fn solve_component<'db>(
    db: &'db dyn Db,
    equations: &FxHashMap<ProjectionType<'db>, Equation<'db>>,
    component: &[ProjectionType<'db>],
    solutions: &mut FxHashMap<ProjectionType<'db>, Type<'db>>,
) {
    let component_set = component.iter().copied().collect::<FxHashSet<_>>();
    let mut bases = Vec::new();
    let mut guarded = false;

    for projection in component {
        let Some(equation) = equations.get(projection) else {
            continue;
        };
        let rhs =
            substitute_external_dependencies(db, equation.rhs, equation, &component_set, solutions);
        let term = remove_component_dependencies(db, rhs, &component_set);
        guarded |= term.guarded;
        if let Some(base) = term.base {
            bases.push(base);
        }
    }

    let base = (!bases.is_empty()).then(|| UnionType::from_elements_cycle_recovery(db, bases));
    for projection in component {
        let solution = match (base, guarded) {
            (Some(base), true) => UnionType::from_elements_cycle_recovery(
                db,
                [base, Type::Divergent(projection.root(db))],
            ),
            (Some(base), false) => base,
            (None, _) => Type::Divergent(projection.root(db)),
        };
        debug_assert!(
            !solution.contains_projection(db, *projection) || guarded,
            "projection solution still contains itself without recursive widening"
        );
        solutions.insert(*projection, solution);
    }
}

fn substitute_external_dependencies<'db>(
    db: &'db dyn Db,
    rhs: Type<'db>,
    equation: &Equation<'db>,
    component: &FxHashSet<ProjectionType<'db>>,
    solutions: &FxHashMap<ProjectionType<'db>, Type<'db>>,
) -> Type<'db> {
    let mut replacements = FxHashMap::default();
    for dependency in &equation.dependencies {
        if component.contains(dependency) {
            continue;
        }
        let replacement = solutions
            .get(dependency)
            .copied()
            .unwrap_or_else(|| Type::Divergent(dependency.root(db)));
        replacements.insert(*dependency, replacement);
    }
    rhs.replace_projections(db, &replacements)
}

#[derive(Debug, Clone, Copy)]
struct ComponentTerm<'db> {
    base: Option<Type<'db>>,
    guarded: bool,
}

fn remove_component_dependencies<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    component: &FxHashSet<ProjectionType<'db>>,
) -> ComponentTerm<'db> {
    let mut bases = Vec::new();
    let mut guarded = false;

    for element in union_elements(db, ty) {
        match element {
            Type::Projection(projection) if component.contains(&projection) => {}
            Type::Divergent(root) if component_contains_root(db, component, root) => {}
            _ if contains_component_projection(db, element, component) => {
                guarded = true;
            }
            _ => {
                bases.push(element);
            }
        }
    }

    ComponentTerm {
        base: (!bases.is_empty()).then(|| UnionType::from_elements_cycle_recovery(db, bases)),
        guarded,
    }
}

fn component_contains_root<'db>(
    db: &'db dyn Db,
    component: &FxHashSet<ProjectionType<'db>>,
    root: DivergentType,
) -> bool {
    component
        .iter()
        .any(|projection| projection.root(db).same_marker(root))
}

fn union_elements<'db>(db: &'db dyn Db, ty: Type<'db>) -> Box<[Type<'db>]> {
    match ty {
        Type::Union(union) => union.elements(db).to_vec().into_boxed_slice(),
        _ => Box::new([ty]),
    }
}

fn contains_component_projection<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    component: &FxHashSet<ProjectionType<'db>>,
) -> bool {
    crate::types::visitor::any_over_type(
        db,
        ty,
        false,
        |nested| matches!(nested, Type::Projection(projection) if component.contains(&projection)),
    )
}

fn components_are_dependency_first<'db>(
    components: &[Vec<ProjectionType<'db>>],
    equations: &FxHashMap<ProjectionType<'db>, Equation<'db>>,
) -> bool {
    let mut component_index = FxHashMap::default();
    for (index, component) in components.iter().enumerate() {
        for projection in component {
            component_index.insert(*projection, index);
        }
    }

    for (index, component) in components.iter().enumerate() {
        for projection in component {
            let Some(equation) = equations.get(projection) else {
                continue;
            };
            for dependency in &equation.dependencies {
                let Some(dependency_index) = component_index.get(dependency).copied() else {
                    continue;
                };
                if dependency_index != index && dependency_index > index {
                    return false;
                }
            }
        }
    }
    true
}

struct Tarjan<'db, 'a> {
    equations: &'a FxHashMap<ProjectionType<'db>, Equation<'db>>,
    next_index: usize,
    stack: Vec<ProjectionType<'db>>,
    on_stack: FxHashSet<ProjectionType<'db>>,
    indices: FxHashMap<ProjectionType<'db>, usize>,
    lowlinks: FxHashMap<ProjectionType<'db>, usize>,
    components: Vec<Vec<ProjectionType<'db>>>,
}

impl<'db, 'a> Tarjan<'db, 'a> {
    fn new(equations: &'a FxHashMap<ProjectionType<'db>, Equation<'db>>) -> Self {
        Self {
            equations,
            next_index: 0,
            stack: Vec::new(),
            on_stack: FxHashSet::default(),
            indices: FxHashMap::default(),
            lowlinks: FxHashMap::default(),
            components: Vec::new(),
        }
    }

    fn components(mut self) -> Vec<Vec<ProjectionType<'db>>> {
        for projection in self.equations.keys().copied() {
            if !self.indices.contains_key(&projection) {
                self.connect(projection);
            }
        }
        self.components
    }

    fn connect(&mut self, projection: ProjectionType<'db>) {
        let index = self.next_index;
        self.next_index += 1;
        self.indices.insert(projection, index);
        self.lowlinks.insert(projection, index);
        self.stack.push(projection);
        self.on_stack.insert(projection);

        if let Some(equation) = self.equations.get(&projection) {
            for dependency in &equation.dependencies {
                if !self.equations.contains_key(dependency) {
                    continue;
                }

                if !self.indices.contains_key(dependency) {
                    self.connect(*dependency);
                    if let Some(dependency_lowlink) = self.lowlinks.get(dependency).copied()
                        && let Some(lowlink) = self.lowlinks.get_mut(&projection)
                    {
                        *lowlink = (*lowlink).min(dependency_lowlink);
                    }
                } else if self.on_stack.contains(dependency)
                    && let Some(dependency_index) = self.indices.get(dependency).copied()
                    && let Some(lowlink) = self.lowlinks.get_mut(&projection)
                {
                    *lowlink = (*lowlink).min(dependency_index);
                }
            }
        }

        if self.lowlinks.get(&projection) == self.indices.get(&projection) {
            self.finish_component(projection);
        }
    }

    fn finish_component(&mut self, projection: ProjectionType<'db>) {
        let mut component = Vec::new();
        while let Some(member) = self.stack.pop() {
            self.on_stack.remove(&member);
            component.push(member);
            if member == projection {
                break;
            }
        }
        self.components.push(component);
    }
}

#[cfg(test)]
mod tests {
    use salsa::plumbing::Id;

    use crate::db::tests::setup_db;
    use crate::types::KnownClass;

    use super::*;

    #[test]
    fn strongly_connected_projection_equations_share_a_solution() {
        let db = setup_db();
        let root_p = Type::divergent(Id::from_bits(1))
            .as_divergent()
            .expect("divergent type should expose its marker");
        let root_q = Type::divergent(Id::from_bits(2))
            .as_divergent()
            .expect("divergent type should expose its marker");
        let projection_p = ProjectionType::new(
            &db,
            root_p,
            vec![ProjectionOp::AwaitResult].into_boxed_slice(),
        );
        let projection_q = ProjectionType::new(
            &db,
            root_q,
            vec![ProjectionOp::AwaitResult].into_boxed_slice(),
        );

        let int = KnownClass::Int.to_instance(&db);
        let str = KnownClass::Str.to_instance(&db);
        let expected = UnionType::from_elements_cycle_recovery(&db, [int, str]);
        let equations = FxHashMap::from_iter([
            (
                projection_p,
                Equation {
                    rhs: UnionType::from_elements_cycle_recovery(
                        &db,
                        [Type::Projection(projection_q), int],
                    ),
                    dependencies: FxOrderSet::from_iter([projection_q]),
                },
            ),
            (
                projection_q,
                Equation {
                    rhs: UnionType::from_elements_cycle_recovery(
                        &db,
                        [Type::Projection(projection_p), str],
                    ),
                    dependencies: FxOrderSet::from_iter([projection_p]),
                },
            ),
        ]);

        let solutions = solve_equations(&db, &equations);

        let solution_p = solutions
            .get(&projection_p)
            .copied()
            .expect("projection p should have a solution");
        let solution_q = solutions
            .get(&projection_q)
            .copied()
            .expect("projection q should have a solution");
        assert!(solution_p.is_equivalent_to(&db, expected));
        assert!(solution_q.is_equivalent_to(&db, expected));
    }
}
