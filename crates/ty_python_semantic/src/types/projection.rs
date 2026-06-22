use std::cell::RefCell;

use ruff_python_ast as ast;
use rustc_hash::{FxHashMap, FxHashSet};
use ty_python_core::EvaluationMode;

use crate::types::tuple::{TupleLength, TupleSpec, VariableLengthTuple};
use crate::types::visitor::{TypeCollector, TypeVisitor, walk_type_with_recursion_guard};
use crate::types::{
    ApplyTypeMappingVisitor, DivergentType, Type, TypeContext, TypeMapping, UnionBuilder, UnionType,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub enum ProjectionUnpackTarget {
    Prefix { index: u32 },
    Starred,
    Suffix { index: u32 },
}

impl<'db> ProjectionOp<'db> {
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
            Self::IterItem { .. } | Self::UnpackExact { .. } | Self::UnpackStarred { .. } => self,
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

    pub(crate) fn apply_path(self, db: &'db dyn Db, mut ty: Type<'db>) -> Type<'db> {
        for op in self.path(db) {
            ty = apply_projection_op(db, ty, *op);
        }
        ty
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
            | ProjectionOp::UnpackStarred { .. } => {}
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ProjectionRecoverySlot<'db> {
    pub(crate) previous: Option<Type<'db>>,
    pub(crate) joined: Type<'db>,
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

    let mut pending = collect_slot_projections(db, slots, recovery_roots);
    if pending.is_empty() {
        return slots
            .iter()
            .map(|slot| normalize_recovered_type(db, slot.joined, cycle))
            .collect();
    }

    let candidates = root_candidates(db, slots, recovery_roots);
    let mut equations = FxHashMap::default();
    let mut index = 0;
    while let Some(&projection) = pending.get_index(index) {
        index += 1;
        if equations.contains_key(&projection) {
            continue;
        }

        let mut rhs = if let Some(candidate) = candidates.get(&projection.root(db)) {
            projection.apply_path(db, *candidate)
        } else {
            Type::Divergent(projection.root(db))
        };
        rhs = replace_expansive_self_dependencies(db, rhs, projection, recovery_roots);
        let dependencies = collect_active_projections(db, rhs, recovery_roots);
        pending.extend(dependencies.iter().copied());
        equations.insert(projection, Equation { rhs, dependencies });
    }

    let solutions = solve_equations(db, &equations);
    slots
        .iter()
        .map(|slot| {
            if collect_active_projections(db, slot.joined, recovery_roots).is_empty() {
                return normalize_recovered_type(db, slot.joined, cycle);
            }

            let recovered = slot
                .joined
                .replace_projections(db, &solutions)
                .replace_active_projections_with_roots(db, recovery_roots);
            normalize_recovered_type(db, recovered, cycle)
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
        let Some(previous) = slot.previous else {
            continue;
        };
        let Some(root) = unique_active_root(db, previous, active_roots) else {
            continue;
        };
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

fn replace_expansive_self_dependencies<'db>(
    db: &'db dyn Db,
    rhs: Type<'db>,
    projection: ProjectionType<'db>,
    active_roots: &[DivergentType],
) -> Type<'db> {
    let replacements = collect_active_projections(db, rhs, active_roots)
        .into_iter()
        .filter(|dependency| is_expansive_self_dependency(db, projection, *dependency))
        .map(|dependency| (dependency, Type::Divergent(dependency.root(db))))
        .collect::<FxHashMap<_, _>>();
    rhs.replace_projections(db, &replacements)
}

fn is_expansive_self_dependency<'db>(
    db: &'db dyn Db,
    projection: ProjectionType<'db>,
    dependency: ProjectionType<'db>,
) -> bool {
    projection.root(db).same_marker(dependency.root(db))
        && dependency.path(db).len() > projection.path(db).len()
        && dependency.path(db).starts_with(projection.path(db))
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

fn apply_projection_op<'db>(db: &'db dyn Db, ty: Type<'db>, op: ProjectionOp<'db>) -> Type<'db> {
    if let Some(projected) = ty.project_cycle(db, op) {
        return projected;
    }

    match op {
        ProjectionOp::Subscript {
            slice,
            expr_context,
        } => ty
            .subscript(db, slice, expr_context)
            .unwrap_or_else(|err| err.result_type()),
        ProjectionOp::IterItem { is_async } => {
            let mode = if is_async {
                EvaluationMode::Async
            } else {
                EvaluationMode::Sync
            };
            ty.try_iterate_with_mode(db, mode).map_or_else(
                |err| err.fallback_element_type(db),
                |tuple| tuple.homogeneous_element_type(db),
            )
        }
        ProjectionOp::UnpackExact { len, index } => {
            apply_unpack_exact_projection(db, ty, len, index)
        }
        ProjectionOp::UnpackStarred {
            before,
            after,
            target,
        } => apply_unpack_starred_projection(db, ty, before, after, target),
        ProjectionOp::Binary {
            op,
            other,
            is_reverse,
        } => {
            let (left, right) = if is_reverse { (other, ty) } else { (ty, other) };
            Type::try_call_bin_op_return_type(db, left, op, right).unwrap_or_else(Type::unknown)
        }
    }
}

fn apply_unpack_exact_projection<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    len: u32,
    index: u32,
) -> Type<'db> {
    if let Type::Union(union) = ty {
        return UnionType::from_elements_cycle_recovery(
            db,
            union
                .elements(db)
                .iter()
                .map(|element| apply_unpack_exact_projection(db, *element, len, index)),
        );
    }

    let Ok(index) = usize::try_from(index) else {
        return Type::unknown();
    };
    let Ok(len) = usize::try_from(len) else {
        return Type::unknown();
    };

    ty.try_iterate(db).map_or_else(
        |err| err.fallback_element_type(db),
        |tuple| {
            let Some(fixed) = tuple.as_fixed_length() else {
                return tuple.homogeneous_element_type(db);
            };
            if fixed.len() != len {
                return tuple.homogeneous_element_type(db);
            }
            fixed
                .all_elements()
                .get(index)
                .copied()
                .unwrap_or_else(Type::unknown)
        },
    )
}

fn apply_unpack_starred_projection<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    before: u32,
    after: u32,
    target: ProjectionUnpackTarget,
) -> Type<'db> {
    if let Type::Union(union) = ty {
        return UnionType::from_elements_cycle_recovery(
            db,
            union.elements(db).iter().map(|element| {
                apply_unpack_starred_projection(db, *element, before, after, target)
            }),
        );
    }

    let Ok(before) = usize::try_from(before) else {
        return Type::unknown();
    };
    let Ok(after) = usize::try_from(after) else {
        return Type::unknown();
    };

    ty.try_iterate(db).map_or_else(
        |err| err.fallback_element_type(db),
        |tuple| {
            let Ok(TupleSpec::Variable(tuple)) =
                tuple.resize(db, TupleLength::Variable(before, after))
            else {
                return Type::unknown();
            };

            match target {
                ProjectionUnpackTarget::Prefix { index } => usize::try_from(index)
                    .ok()
                    .and_then(|index| tuple.prefix_elements().get(index).copied())
                    .unwrap_or_else(Type::unknown),
                ProjectionUnpackTarget::Starred => tuple.variable(),
                ProjectionUnpackTarget::Suffix { index } => usize::try_from(index)
                    .ok()
                    .and_then(|index| tuple.suffix_elements().get(index).copied())
                    .unwrap_or_else(Type::unknown),
            }
        },
    )
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
    let mut builder = UnionBuilder::new(db).cycle_recovery(true);
    let mut has_base = false;
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
            builder = builder.add(base);
            has_base = true;
        }
    }

    let base = has_base.then(|| builder.build());
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
    let mut builder = UnionBuilder::new(db).cycle_recovery(true);
    let mut has_base = false;
    let mut guarded = false;

    for element in union_elements(db, ty) {
        match element {
            Type::Projection(projection) if component.contains(&projection) => {}
            _ if contains_component_projection(db, element, component) => {
                guarded = true;
            }
            _ => {
                builder = builder.add(element);
                has_base = true;
            }
        }
    }

    ComponentTerm {
        base: has_base.then(|| builder.build()),
        guarded,
    }
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
