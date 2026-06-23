//! Cycle-recovery projections.
//!
//! Recursive inference can encounter operations on a value whose final type is
//! still being inferred. This module records those operations as projection
//! paths, then solves them once the recovered recursive type exposes enough
//! concrete container structure.

use std::cell::RefCell;

use super::{DivergentType, KnownClass, TupleSpec, Type, UnionBuilder, UnionType};
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::tuple::TupleType;
use crate::types::visitor::any_over_type;
use crate::{Db, FxIndexMap, FxIndexSet};

mod artifact;
mod container;
mod equation;
mod evidence;
mod operation;
mod recovery;
mod term;

use artifact::ProjectionPath;
pub use artifact::ProjectionType;
use container::ProjectionContainer;
pub(crate) use equation::ProjectionSolutions;
use equation::{CycleRootSet, ProjectionEquationSystem, ProjectionVar};
pub(crate) use evidence::ProjectionEvidenceSet;
pub(crate) use operation::ProjectionResult;
pub(crate) use recovery::ProjectionRecoveryBuilder;
use term::ProjectionTerm;

impl<'db> Type<'db> {
    fn top_level_cycle_artifact_root(self, db: &'db dyn Db) -> Option<DivergentType> {
        match self {
            Type::Divergent(root) => Some(root),
            Type::Projection(projection) => Some(projection.root(db)),
            _ => None,
        }
    }

    pub(crate) const fn is_cycle_artifact(&self) -> bool {
        matches!(self, Type::Divergent(_) | Type::Projection(_))
    }

    /// Inference-time API: returns whether this type still contains a cycle artifact.
    pub(crate) fn contains_cycle_artifact(self, db: &'db dyn Db) -> bool {
        self.contains_nested_cycle_artifact(db, true)
    }

    fn has_top_level_cycle_artifact(self, db: &'db dyn Db) -> bool {
        match self {
            Type::Divergent(_) | Type::Projection(_) => true,
            Type::Union(union) => union.elements(db).iter().any(Self::is_cycle_artifact),
            _ => false,
        }
    }

    /// Returns whether applying a projection operation can observe a cycle artifact.
    fn needs_projection_operation(self, db: &'db dyn Db) -> bool {
        self.contains_nested_cycle_artifact(db, true)
    }

    /// Returns `true` if both types originate from the same cycle root, regardless
    /// of whether either occurrence is a direct marker or a projection of it.
    pub(crate) fn same_divergent_marker(self, db: &'db dyn Db, other: Type<'db>) -> bool {
        match (self, other) {
            (Type::Divergent(left), Type::Divergent(right)) => left.same_marker(right),
            (Type::Projection(left), Type::Divergent(right))
            | (Type::Divergent(right), Type::Projection(left)) => left.root(db).same_marker(right),
            (Type::Projection(left), Type::Projection(right)) => {
                left.root(db).same_marker(right.root(db))
            }
            _ => false,
        }
    }

    /// Cycle-recovery-time API: legacy root-local fallback for projection recovery.
    ///
    /// Result-level recovery uses [`ProjectionRecoveryBuilder`] and solves all visible roots
    /// together. This fallback remains for type-local normalization paths that do not have a
    /// result-wide slot set.
    pub(crate) fn try_projection_cycle_normalized(
        self,
        db: &'db dyn Db,
        root: DivergentType,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
    ) -> Option<Self> {
        let paths = self.projection_ops(db, root);
        if paths.is_empty() {
            return None;
        }

        self.try_container_projection_cycle_normalized(db, root, &paths, evidence)
    }

    /// Cycle-recovery-time API: solves projections explainable by top-level containers.
    ///
    /// The solver works in four steps:
    ///
    /// 1. Split the candidate recursive type into its top-level union arms and
    ///    reuse the collected projection paths rooted at `root`.
    ///    For example, for a type equation `D = tuple[int] | tuple[Projection_{Subscript[0]}(D)]`,
    ///    * `self = tuple[int] | tuple[Projection_{Subscript[0]}(D)]`
    ///    * `elements = [tuple[int], tuple[Projection_{Subscript[0]}(D)]]`
    ///    * `paths = [Subscript[0]]`
    /// 2. Treat non-root union arms as container evidence. Each supported arm
    ///    must be able to project every collected path.
    ///    Container operations are evaluated structurally; operations whose
    ///    result depends on member or call inference are not recomputed here.
    ///    Here, both tuple arms become `containers`: projecting the first arm
    ///    by `Subscript[0]` yields `int`, while projecting the second yields
    ///    `Projection_{Subscript[0]}(D)`. These terms are stored by path in
    ///    `terms_by_op`.
    ///    * `containers = [tuple[int], tuple[Projection_{Subscript[0]}(D)]]`
    ///    * `terms_by_op = [(Subscript[0], [Exact(int), Exact(Projection_{Subscript[0]}(D))])]`
    /// 3. Lower projected terms into an equation system. Each projection path
    ///    is a variable; closed terms become productive bases, flat projection
    ///    occurrences become graph dependencies, and same-root occurrences
    ///    below constructors mark the equation as divergent.
    ///    In the example, `Subscript[0]` has one productive base `int` and a
    ///    flat self dependency on `Subscript[0]`.
    /// 4. Rebuild the original top-level arms with every cycle artifact replaced
    ///    by the SCC solution for its projection path.
    ///    Rebuilding turns `tuple[Projection_{Subscript[0]}(D)]` into
    ///    `tuple[int]`, so the candidate normalizes to `tuple[int]`.
    ///
    /// Returning `None` means that this recovery step cannot make progress
    /// without losing information; Salsa cycle recovery can then keep iterating
    /// toward a wider fixed point.
    fn try_container_projection_cycle_normalized(
        self,
        db: &'db dyn Db,
        root: DivergentType,
        paths: &[ProjectionPath<'db>],
        evidence: Option<&ProjectionEvidenceSet<'db>>,
    ) -> Option<Self> {
        let elements = self.top_level_projection_union_elements(db);
        let mut containers = Vec::new();

        for element in &elements {
            if element.same_divergent_marker(db, Type::Divergent(root)) {
                continue;
            }

            let container = ProjectionContainer::try_from(db, root, *element, evidence)?;
            containers.push(container);
        }

        if containers.is_empty() {
            return None;
        }

        let mut terms_by_op = paths
            .iter()
            .cloned()
            .map(|path| (path, Vec::new()))
            .collect::<FxIndexMap<_, _>>();
        for container in &containers {
            container.collect_projection_terms(db, root, evidence, &mut terms_by_op)?;
        }

        let solutions =
            ProjectionEquationSystem::from_terms_by_op(db, root, &terms_by_op)?.solve(db)?;
        let solved_ops = paths
            .iter()
            .map(|path| {
                let var = ProjectionVar {
                    root,
                    path: path.clone(),
                };
                Some((path.clone(), solutions.solved_type(db, &var)?))
            })
            .collect::<Option<FxIndexMap<_, _>>>()?;

        let elements = elements
            .into_iter()
            .filter(|element| {
                !matches!(element, Type::Divergent(divergent) if divergent.same_marker(root))
            })
            .map(|element| {
                Some((
                    element.replace_solved_projection_artifacts(
                        db,
                        root,
                        &solved_ops,
                        evidence,
                    )?,
                    element.mentions_cycle_artifact_direct(db, root),
                ))
            })
            .collect::<Option<Vec<_>>>()?;

        let ty = Self::union_projection_cycle_recovery(db, elements);
        debug_assert!(
            !ty.mentions_projection_artifact_in_roots(db, &CycleRootSet::single(root)),
            "projection recovery must not leave unsolved projection artifacts"
        );
        Some(ty)
    }

    fn union_projection_cycle_recovery(db: &'db dyn Db, elements: Vec<(Self, bool)>) -> Self {
        if let Some(ty) = Self::try_union_fixed_length_tuples_cycle_recovery(db, &elements) {
            return ty;
        }

        if let Some(ty) = Self::try_union_direct_instances_cycle_recovery(db, &elements) {
            return ty;
        }

        UnionType::from_elements_cycle_recovery(db, elements.into_iter().map(|(ty, _)| ty))
    }

    fn try_union_fixed_length_tuples_cycle_recovery(
        db: &'db dyn Db,
        elements: &[(Self, bool)],
    ) -> Option<Self> {
        let [(first, _), rest @ ..] = elements else {
            return None;
        };
        let first_spec = first.exact_tuple_instance_spec(db)?;
        let first_tuple = first_spec.as_ref().as_fixed_length()?;
        let mut element_builders = first_tuple
            .iter_all_elements()
            .map(|element| {
                UnionBuilder::new(db)
                    .cycle_recovery(true)
                    .recursively_defined(RecursivelyDefined::Yes)
                    .add(element)
            })
            .collect::<Vec<_>>();

        for (element, _) in rest {
            let spec = element.exact_tuple_instance_spec(db)?;
            let tuple = spec.as_ref().as_fixed_length()?;
            if tuple.len() != element_builders.len() {
                return None;
            }

            for (builder, element) in element_builders.iter_mut().zip(tuple.iter_all_elements()) {
                builder.add_in_place(element);
            }
        }

        Some(Type::heterogeneous_tuple(
            db,
            element_builders.into_iter().map(UnionBuilder::build),
        ))
    }

    fn try_union_direct_instances_cycle_recovery(
        db: &'db dyn Db,
        elements: &[(Self, bool)],
    ) -> Option<Self> {
        let [(first, first_is_recursive), rest @ ..] = elements else {
            return None;
        };
        let (class, specialization) = first.direct_class_specialization(db)?;
        if class.is_known(db, KnownClass::Tuple) {
            return None;
        }
        let mut recursive_count = usize::from(*first_is_recursive);
        let mut seed_count = usize::from(!*first_is_recursive);
        let mut argument_builders = specialization
            .types(db)
            .iter()
            .map(|argument| {
                UnionBuilder::new(db)
                    .cycle_recovery(true)
                    .recursively_defined(RecursivelyDefined::Yes)
                    .add(*argument)
            })
            .collect::<Vec<_>>();

        for (element, is_recursive) in rest {
            let (element_class, specialization) = element.direct_class_specialization(db)?;
            if element_class != class {
                return None;
            }

            let arguments = specialization.types(db);
            if arguments.len() != argument_builders.len() {
                return None;
            }

            recursive_count += usize::from(*is_recursive);
            seed_count += usize::from(!*is_recursive);

            for (builder, argument) in argument_builders.iter_mut().zip(arguments) {
                builder.add_in_place(*argument);
            }
        }

        // For invariant containers, argument-wise union is only used as widening for one recursive
        // chain. Multiple seed arms remain a union unless normal union simplification merges them.
        if recursive_count == 0 || seed_count != 1 {
            return None;
        }

        let arguments = argument_builders
            .into_iter()
            .map(UnionBuilder::build)
            .collect::<Vec<_>>();

        Type::from(class.apply_specialization(db, |generic_context| {
            generic_context.specialize(db, arguments)
        }))
        .to_instance(db)
    }

    fn top_level_projection_union_elements(self, db: &'db dyn Db) -> Vec<Self> {
        match self {
            Type::Union(union) => union.elements(db).to_vec(),
            _ => vec![self],
        }
    }

    /// Solves the candidate terms for one projection path.
    fn solve_projection_terms(
        db: &'db dyn Db,
        root: DivergentType,
        path: &ProjectionPath<'db>,
        terms: &[ProjectionTerm<'db>],
    ) -> Option<Self> {
        let mut terms_by_op = FxIndexMap::default();
        terms_by_op.insert(path.clone(), terms.to_vec());
        let var = ProjectionVar {
            root,
            path: path.clone(),
        };
        ProjectionEquationSystem::from_terms_by_op(db, root, &terms_by_op)?
            .solve(db)?
            .solved_type(db, &var)
    }

    fn projection_ops(self, db: &'db dyn Db, root: DivergentType) -> Vec<ProjectionPath<'db>> {
        let mut paths = Vec::new();
        let demands = self.projection_demands(db);
        for (candidate_root, path) in demands {
            if candidate_root.same_marker(root) && !paths.contains(&path) {
                paths.push(path);
            }
        }
        paths
    }

    fn projection_demands(self, db: &'db dyn Db) -> Vec<(DivergentType, ProjectionPath<'db>)> {
        let demands = RefCell::<Vec<(DivergentType, ProjectionPath<'db>)>>::new(Vec::new());
        any_over_type(db, self, false, |nested| {
            if let Type::Projection(projection) = nested {
                let mut demands = demands.borrow_mut();
                let root = projection.root(db);
                let path = projection.path(db);
                if !demands.iter().any(|(candidate_root, candidate_path)| {
                    candidate_root.same_marker(root) && candidate_path == &path
                }) {
                    demands.push((root, path));
                }
            }
            false
        });
        demands.into_inner()
    }

    fn cycle_artifact_roots(self, db: &'db dyn Db) -> Vec<DivergentType> {
        let mut roots = Vec::new();
        self.collect_cycle_artifact_roots(db, &mut roots, true);
        roots
    }

    fn projection_artifact_roots(self, db: &'db dyn Db) -> Vec<DivergentType> {
        let mut roots = Vec::new();
        // Bare `Divergent` inside containers appears in recursive aliases too. Nested projection
        // recovery only starts from an already-recorded projection demand.
        self.collect_cycle_artifact_roots(db, &mut roots, false);
        roots
    }

    fn collect_cycle_artifact_roots(
        self,
        db: &'db dyn Db,
        roots: &mut Vec<DivergentType>,
        include_divergent: bool,
    ) {
        match self {
            Type::Divergent(root) if include_divergent => {
                Self::push_cycle_artifact_root(roots, root);
            }
            Type::Projection(projection) => {
                Self::push_cycle_artifact_root(roots, projection.root(db));
            }
            Type::Union(union) => {
                for element in union.elements(db) {
                    element.collect_cycle_artifact_roots(db, roots, include_divergent);
                }
            }
            Type::NominalInstance(_) => {
                if let Some(spec) = self.exact_tuple_instance_spec(db) {
                    for element in spec.as_ref().iter_all_elements() {
                        element.collect_cycle_artifact_roots(db, roots, include_divergent);
                    }
                } else if let Some((_, specialization)) = self.direct_class_specialization(db) {
                    for argument in specialization.types(db) {
                        argument.collect_cycle_artifact_roots(db, roots, include_divergent);
                    }
                }
            }
            _ => {}
        }
    }

    fn contains_nested_cycle_artifact(self, db: &'db dyn Db, include_divergent: bool) -> bool {
        match self {
            Type::Divergent(_) => include_divergent,
            Type::Projection(_) => true,
            Type::Union(union) => union
                .elements(db)
                .iter()
                .any(|element| element.contains_nested_cycle_artifact(db, include_divergent)),
            Type::NominalInstance(_) => {
                if let Some(spec) = self.exact_tuple_instance_spec(db) {
                    spec.as_ref().iter_all_elements().any(|element| {
                        element.contains_nested_cycle_artifact(db, include_divergent)
                    })
                } else if let Some((_, specialization)) = self.direct_class_specialization(db) {
                    specialization.types(db).iter().any(|argument| {
                        argument.contains_nested_cycle_artifact(db, include_divergent)
                    })
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn push_cycle_artifact_root(roots: &mut Vec<DivergentType>, root: DivergentType) {
        if !roots.iter().any(|candidate| candidate.same_marker(root)) {
            roots.push(root);
        }
    }

    fn mentions_cycle_artifact_direct(self, db: &'db dyn Db, root: DivergentType) -> bool {
        let roots = self.cycle_artifact_roots(db);
        roots.iter().any(|candidate| candidate.same_marker(root))
    }

    fn solved_projection_type(
        solved_ops: &FxIndexMap<ProjectionPath<'db>, Type<'db>>,
        path: &ProjectionPath<'db>,
    ) -> Option<Self> {
        solved_ops.get(path).copied()
    }

    fn replace_solved_projection_artifacts(
        self,
        db: &'db dyn Db,
        root: DivergentType,
        solved_ops: &FxIndexMap<ProjectionPath<'db>, Type<'db>>,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
    ) -> Option<Self> {
        if !self.mentions_cycle_artifact(db, root) {
            return Some(self);
        }

        if let Type::Projection(projection) = self
            && projection.root(db).same_marker(root)
        {
            return Self::solved_projection_type(solved_ops, &projection.path(db));
        }

        if let Type::Union(union) = self {
            let elements = union
                .elements(db)
                .iter()
                .map(|element| {
                    element.replace_solved_projection_artifacts(db, root, solved_ops, evidence)
                })
                .collect::<Option<Vec<_>>>()?;
            return Some(UnionType::from_elements_cycle_recovery(db, elements));
        }

        if let Some(container) = ProjectionContainer::try_from(db, root, self, evidence) {
            return container.into_type(db, root, solved_ops, evidence);
        }

        let paths = self.projection_ops(db, root);
        match paths.as_slice() {
            [path] => Self::solved_projection_type(solved_ops, path),
            _ => None,
        }
    }

    fn replace_projection_artifacts_with_root(
        self,
        db: &'db dyn Db,
        root: DivergentType,
    ) -> Option<Self> {
        let paths = self.projection_ops(db, root);
        if paths.is_empty() {
            return Some(self);
        }

        let solved_ops = paths
            .into_iter()
            .map(|path| (path, Type::Divergent(root)))
            .collect::<FxIndexMap<_, _>>();
        self.replace_solved_projection_artifacts(db, root, &solved_ops, None)
    }

    fn mentions_cycle_artifact(self, db: &'db dyn Db, root: DivergentType) -> bool {
        any_over_type(db, self, false, |ty| match ty {
            Type::Divergent(divergent) => divergent.same_marker(root),
            Type::Projection(projection) => projection.root(db).same_marker(root),
            _ => false,
        })
    }

    fn mentions_matching_projection(
        self,
        db: &'db dyn Db,
        root: DivergentType,
        path: &ProjectionPath<'db>,
    ) -> bool {
        any_over_type(db, self, false, |ty| match ty {
            Type::Projection(projection) => {
                projection.root(db).same_marker(root) && projection.path(db).eq(path)
            }
            _ => false,
        })
    }

    fn is_matching_projection(
        self,
        db: &'db dyn Db,
        root: DivergentType,
        path: &ProjectionPath<'db>,
    ) -> bool {
        matches!(
            self,
            Type::Projection(projection)
                if projection.root(db).same_marker(root) && projection.path(db).eq(path)
        )
    }

    fn matching_projection_narrowing_var_multi(
        self,
        db: &'db dyn Db,
        roots: &CycleRootSet,
    ) -> Option<ProjectionVar<'db>> {
        let Type::Intersection(intersection) = self else {
            return None;
        };

        let mut dependency = None;
        for positive in intersection.positive(db) {
            if let Type::Projection(projection) = positive {
                let root = projection.root(db);
                if !roots.contains(root) {
                    return None;
                }
                let var = ProjectionVar {
                    root,
                    path: projection.path(db),
                };
                if dependency.as_ref().is_some_and(|existing| existing != &var) {
                    return None;
                }
                dependency = Some(var);
            } else if positive.mentions_any_cycle_artifact(db) {
                return None;
            }
        }

        for negative in intersection.negative(db) {
            if negative.mentions_any_cycle_artifact(db) {
                return None;
            }
        }

        dependency
    }

    fn mentions_cycle_artifact_in_roots(self, db: &'db dyn Db, roots: &CycleRootSet) -> bool {
        any_over_type(db, self, false, |ty| match ty {
            Type::Divergent(root) => roots.contains(root),
            Type::Projection(projection) => roots.contains(projection.root(db)),
            _ => false,
        })
    }

    fn mentions_projection_artifact_in_roots(self, db: &'db dyn Db, roots: &CycleRootSet) -> bool {
        any_over_type(db, self, false, |ty| match ty {
            Type::Projection(projection) => roots.contains(projection.root(db)),
            _ => false,
        })
    }

    fn mentions_cycle_artifact_outside_roots(self, db: &'db dyn Db, roots: &CycleRootSet) -> bool {
        any_over_type(db, self, false, |ty| match ty {
            Type::Divergent(root) => !roots.contains(root),
            Type::Projection(projection) => !roots.contains(projection.root(db)),
            _ => false,
        })
    }

    fn mentions_divergent_in_roots(self, db: &'db dyn Db, roots: &CycleRootSet) -> bool {
        any_over_type(db, self, false, |ty| match ty {
            Type::Divergent(root) => roots.contains(root),
            _ => false,
        })
    }

    fn mentions_projection_var_in(
        self,
        db: &'db dyn Db,
        vars: &FxIndexSet<ProjectionVar<'db>>,
    ) -> bool {
        any_over_type(db, self, false, |ty| match ty {
            Type::Projection(projection) => vars.contains(&ProjectionVar {
                root: projection.root(db),
                path: projection.path(db),
            }),
            _ => false,
        })
    }

    pub(crate) fn replace_solved_projection_vars(
        self,
        db: &'db dyn Db,
        solutions: &ProjectionSolutions<'db>,
    ) -> Option<Self> {
        let roots = solutions.roots();
        if !self.mentions_cycle_artifact_in_roots(db, &roots) {
            return Some(self);
        }

        if let Type::Divergent(root) = self
            && roots.contains(root)
        {
            return Some(self);
        }

        if let Type::Projection(projection) = self {
            return solutions.solved_type(
                db,
                &ProjectionVar {
                    root: projection.root(db),
                    path: projection.path(db),
                },
            );
        }

        if let Type::Union(union) = self {
            let elements = union
                .elements(db)
                .iter()
                .map(|element| element.replace_solved_projection_vars(db, solutions))
                .collect::<Option<Vec<_>>>()?;
            return Some(UnionType::from_elements_cycle_recovery(db, elements));
        }

        if let Some(spec) = self.exact_tuple_instance_spec(db) {
            return Some(match spec.as_ref() {
                TupleSpec::Fixed(tuple) => {
                    let elements = tuple
                        .iter_all_elements()
                        .map(|element| element.replace_solved_projection_vars(db, solutions))
                        .collect::<Option<Vec<_>>>()?;
                    Type::heterogeneous_tuple(db, elements)
                }
                TupleSpec::Variable(tuple) => {
                    let prefix = tuple
                        .iter_prefix_elements()
                        .map(|element| element.replace_solved_projection_vars(db, solutions))
                        .collect::<Option<Vec<_>>>()?;
                    let variable = tuple
                        .variable()
                        .replace_solved_projection_vars(db, solutions)?;
                    let suffix = tuple
                        .iter_suffix_elements()
                        .map(|element| element.replace_solved_projection_vars(db, solutions))
                        .collect::<Option<Vec<_>>>()?;
                    Type::tuple(TupleType::mixed(db, prefix, variable, suffix))
                }
            });
        }

        if let Some((class, specialization)) = self.direct_class_specialization(db) {
            let arguments = specialization
                .types(db)
                .iter()
                .map(|argument| argument.replace_solved_projection_vars(db, solutions))
                .collect::<Option<Vec<_>>>()?;
            return Type::from(class.apply_specialization(db, |generic_context| {
                generic_context.specialize(db, arguments)
            }))
            .to_instance(db);
        }

        None
    }

    fn mentions_any_cycle_artifact(self, db: &'db dyn Db) -> bool {
        any_over_type(db, self, false, |ty| {
            matches!(ty, Type::Divergent(_) | Type::Projection(_))
        })
    }
}
