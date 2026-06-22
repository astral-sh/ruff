//! Cycle-recovery projections.
//!
//! Recursive inference can encounter operations on a value whose final type is
//! still being inferred. This module records those operations as projection
//! paths, then solves them once the recovered recursive type exposes enough
//! concrete container structure.

use std::cell::RefCell;

use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ty_python_core::EvaluationMode;

use super::{
    DivergentType, KnownClass, MemberLookupPolicy, TupleSpec, Type, UnionBuilder, UnionType,
    subscript::SubscriptError,
};
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::tuple::TupleType;
use crate::types::visitor::any_over_type;
use crate::{Db, FxIndexMap, FxIndexSet};

mod artifact;
mod container;
mod equation;
mod evidence;
mod recovery;
mod term;

pub use artifact::ProjectionType;
use artifact::{
    ProjectionMember, ProjectionMemberName, ProjectionOp, ProjectionPath, ProjectionSubscript,
    StarUnpackPosition, UnpackProjection,
};
use container::ProjectionContainer;
pub(crate) use equation::ProjectionSolutions;
use equation::{CycleRootSet, ProjectionEquationSystem, ProjectionVar};
use evidence::ProjectionEvidenceBuilder;
pub(crate) use evidence::ProjectionEvidenceSet;
pub(crate) use recovery::ProjectionRecoveryBuilder;
use term::ProjectionTerm;

impl<'db> Type<'db> {
    /// Inference-time API: projects an iterable value while recording cycle projection evidence.
    pub(crate) fn try_iter_projection_result_with_mode(
        self,
        db: &'db dyn Db,
        mode: EvaluationMode,
    ) -> Option<ProjectionResult<'db>> {
        let op = ProjectionOp::Iter {
            is_async: mode.is_async(),
        };
        self.try_projection_with_non_cycle_result(db, op, |ty| {
            ty.try_iterate_with_mode(db, mode)
                .ok()
                .map(|tuple| ProjectionTerm::Homogeneous(tuple.homogeneous_element_type(db)))
        })
    }

    /// Inference-time API: projects one target of an exact unpack operation.
    pub(crate) fn try_unpack_projection_result(
        self,
        db: &'db dyn Db,
        len: usize,
        index: usize,
    ) -> Option<ProjectionResult<'db>> {
        let op = ProjectionOp::Unpack(UnpackProjection::Exact { len, index });
        self.try_projection_with_non_cycle_result(db, op, |ty| {
            ProjectionContainer::infer_projection_op(db, ty, op)
        })
    }

    /// Inference-time API: projects one fixed prefix target of a starred unpack operation.
    pub(crate) fn try_star_unpack_prefix_projection_result(
        self,
        db: &'db dyn Db,
        prefix: usize,
        suffix: usize,
        index: usize,
    ) -> Option<ProjectionResult<'db>> {
        let op = ProjectionOp::Unpack(UnpackProjection::Star {
            prefix,
            suffix,
            position: StarUnpackPosition::Prefix(index),
        });
        self.try_projection_with_non_cycle_result(db, op, |ty| {
            ProjectionContainer::infer_projection_op(db, ty, op)
        })
    }

    /// Inference-time API: projects the list-valued rest target of a starred unpack operation.
    pub(crate) fn try_star_unpack_rest_projection_result(
        self,
        db: &'db dyn Db,
        prefix: usize,
        suffix: usize,
    ) -> Option<ProjectionResult<'db>> {
        let op = ProjectionOp::Unpack(UnpackProjection::Star {
            prefix,
            suffix,
            position: StarUnpackPosition::Rest,
        });
        self.try_projection_with_non_cycle_result(db, op, |ty| {
            ProjectionContainer::infer_projection_op(db, ty, op)
        })
    }

    /// Inference-time API: projects one fixed suffix target of a starred unpack operation.
    pub(crate) fn try_star_unpack_suffix_projection_result(
        self,
        db: &'db dyn Db,
        prefix: usize,
        suffix: usize,
        index: usize,
    ) -> Option<ProjectionResult<'db>> {
        let op = ProjectionOp::Unpack(UnpackProjection::Star {
            prefix,
            suffix,
            position: StarUnpackPosition::Suffix(index),
        });
        self.try_projection_with_non_cycle_result(db, op, |ty| {
            ProjectionContainer::infer_projection_op(db, ty, op)
        })
    }

    /// Inference-time API: projects a subscript operation without returning replay evidence.
    pub(crate) fn try_subscript_projection(
        self,
        db: &'db dyn Db,
        slice_ty: Type<'db>,
    ) -> Option<Self> {
        self.try_subscript_projection_result(db, slice_ty)
            .map(ProjectionResult::ty)
    }

    /// Inference-time API: projects a subscript operation while recording cycle projection evidence.
    pub(crate) fn try_subscript_projection_result(
        self,
        db: &'db dyn Db,
        slice_ty: Type<'db>,
    ) -> Option<ProjectionResult<'db>> {
        let subscript = ProjectionSubscript::from_type(db, slice_ty)?;
        let op = ProjectionOp::Subscript(subscript);
        self.try_projection_with_non_cycle_result(db, op, |ty| {
            ty.subscript(db, slice_ty, ast::ExprContext::Load)
                .ok()
                .map(ProjectionTerm::Exact)
        })
    }

    /// Inference-time API: tries ordinary subscript semantics before projection for concrete keys.
    ///
    /// Concrete non-index keys can produce real diagnostics on some union arms, such as
    /// `list[T]["key"]`. Treating those keys as an unknown projection would hide the errors.
    pub(crate) fn try_subscript_without_projection_for_concrete_key(
        self,
        db: &'db dyn Db,
        slice_ty: Type<'db>,
        expr_context: ast::ExprContext,
    ) -> Option<Result<Self, SubscriptError<'db>>> {
        if !matches!(
            ProjectionSubscript::from_type(db, slice_ty)?,
            ProjectionSubscript::KeyType(_)
        ) {
            return None;
        }

        let result = self.subscript_without_projection(db, slice_ty, expr_context);
        match result {
            Ok(ty) if !ty.has_top_level_cycle_artifact(db) => Some(Ok(ty)),
            Err(error) => Some(Err(error)),
            Ok(_) => None,
        }
    }

    /// Inference-time API: projects an attribute lookup while recording cycle projection evidence.
    pub(crate) fn try_member_projection_result(
        self,
        db: &'db dyn Db,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> Option<ProjectionResult<'db>> {
        let op = ProjectionOp::Member(ProjectionMember::new(db, name, policy));
        self.try_projection_with_non_cycle_result(db, op, |ty| {
            ProjectionContainer::infer_member_type_for_type(db, ty, name, policy)
                .map(ProjectionTerm::Exact)
        })
    }

    /// Inference-time API: projects a zero-argument method call.
    pub(crate) fn try_method_call_projection_result(
        self,
        db: &'db dyn Db,
        method_name: &Name,
    ) -> Option<ProjectionResult<'db>> {
        let op = ProjectionOp::CallMethod0(ProjectionMemberName::new(db, method_name));
        self.try_projection_with_non_cycle_result(db, op, |ty| {
            ProjectionContainer::infer_method_call0_type_for_type(db, ty, method_name)
                .map(ProjectionTerm::Exact)
        })
    }

    /// Inference-time API: projects a context-manager enter operation without replay evidence.
    pub(crate) fn try_context_enter_projection(
        self,
        db: &'db dyn Db,
        mode: EvaluationMode,
    ) -> Option<Self> {
        self.try_context_enter_projection_result(db, mode)
            .map(ProjectionResult::ty)
    }

    /// Inference-time API: projects a context-manager enter operation.
    pub(crate) fn try_context_enter_projection_result(
        self,
        db: &'db dyn Db,
        mode: EvaluationMode,
    ) -> Option<ProjectionResult<'db>> {
        let op = ProjectionOp::ContextEnter {
            is_async: mode.is_async(),
        };
        self.try_projection_with_non_cycle_result(db, op, |ty| {
            ty.try_enter_with_mode(db, mode)
                .ok()
                .map(ProjectionTerm::Exact)
        })
    }

    /// Inference-time API: projects the result of awaiting a value.
    pub(crate) fn try_await_projection_result(
        self,
        db: &'db dyn Db,
    ) -> Option<ProjectionResult<'db>> {
        self.try_projection_with_non_cycle_result(db, ProjectionOp::AwaitResult, |ty| {
            ty.try_await(db).ok().map(ProjectionTerm::Exact)
        })
    }

    fn try_projection_result(
        self,
        db: &'db dyn Db,
        op: ProjectionOp<'db>,
    ) -> Option<ProjectionResult<'db>> {
        match self {
            Type::Divergent(root) => Some(ProjectionResult::new(Self::Projection(
                ProjectionType::new(db, root, ProjectionPath::from_op(op)),
            ))),
            Type::Projection(projection) => Some(ProjectionResult::new(Self::Projection(
                projection.append(db, op),
            ))),
            _ => None,
        }
    }

    /// Inference-time helper for applying an operation to a type that may contain cycle markers.
    fn try_projection_with_non_cycle_result(
        self,
        db: &'db dyn Db,
        op: ProjectionOp<'db>,
        mut project_non_cycle: impl FnMut(Self) -> Option<ProjectionTerm<'db>>,
    ) -> Option<ProjectionResult<'db>> {
        if !self.has_top_level_cycle_artifact(db) {
            return self.try_nested_cycle_projection_result(db, op, project_non_cycle);
        }

        let Type::Union(union) = self else {
            return self.try_projection_result(db, op);
        };

        let roots: Vec<DivergentType> = union
            .elements(db)
            .iter()
            .filter_map(|element| element.top_level_cycle_artifact_root(db))
            .fold(Vec::new(), |mut roots, root| {
                if !roots.iter().any(|candidate| candidate.same_marker(root)) {
                    roots.push(root);
                }
                roots
            });

        let mut elements = Vec::new();
        let mut projected_non_cycle_elements = Vec::new();
        let mut projection_evidence = ProjectionEvidenceBuilder::default();
        let path = ProjectionPath::from_op(op);

        for element in union.elements(db).iter().copied() {
            if element.top_level_cycle_artifact_root(db).is_some() {
                continue;
            }

            let term = project_non_cycle(element)?;
            projection_evidence.record_projected_arm(
                db,
                roots.iter().copied(),
                element,
                &path,
                term,
            );
            projected_non_cycle_elements.push((element, term.ty(db)));
        }

        let mut projected_non_cycle_elements = projected_non_cycle_elements.into_iter();
        for element in union.elements(db).iter().copied() {
            if let Some(projected) = element.try_projection_result(db, op) {
                elements.push(projected.ty());
            } else {
                let (original, projected_ty) = projected_non_cycle_elements.next()?;
                debug_assert_eq!(element, original);
                elements.push(projected_ty);
            }
        }

        Some(ProjectionResult {
            ty: UnionType::from_elements_cycle_recovery(db, elements),
            projection_evidence: projection_evidence.finish(db),
        })
    }

    /// Inference-time helper for projection artifacts nested below a top-level non-cycle shape.
    fn try_nested_cycle_projection_result(
        self,
        db: &'db dyn Db,
        op: ProjectionOp<'db>,
        mut project_non_cycle: impl FnMut(Self) -> Option<ProjectionTerm<'db>>,
    ) -> Option<ProjectionResult<'db>> {
        let mut roots = self.projection_artifact_roots(db);
        // Bare divergent roots below a bridge container also need projection evidence. Unpack is
        // excluded because unpacking can be the operation that grows a recursive structure.
        if roots.is_empty() && !matches!(op, ProjectionOp::Unpack(_)) {
            roots = self.cycle_artifact_roots(db);
        }
        let [root] = roots.as_slice() else {
            return self.try_multi_root_nested_cycle_projection_result(
                db,
                op,
                &roots,
                project_non_cycle,
            );
        };

        let elements = self.top_level_projection_union_elements(db);
        let mut projection_evidence = ProjectionEvidenceBuilder::default();
        let path = ProjectionPath::from_op(op);
        let mut terms = vec![None; elements.len()];
        let mut recursive_elements = Vec::new();

        for (index, element) in elements.iter().copied().enumerate() {
            if element.mentions_cycle_artifact_direct(db, *root) {
                recursive_elements.push((index, element));
                continue;
            }

            let term = project_non_cycle(element)?;
            projection_evidence.record_projected_arm(db, [*root], element, &path, term);
            terms[index] = Some(term);
        }

        let evidence = projection_evidence.finish(db);
        for (index, element) in recursive_elements {
            let container = ProjectionContainer::try_from(db, *root, element, evidence.as_ref())?;
            let term = container
                .project_path(db, *root, evidence.as_ref(), &path)
                .or_else(|| {
                    if matches!(op, ProjectionOp::Subscript(_)) {
                        // The subscript path suppresses projection creation, so it can expose a
                        // flat dependency without recursively extending the projection cycle.
                        ProjectionContainer::infer_projection_op(db, element, op)
                    } else {
                        None
                    }
                })?;
            terms[index] = Some(term);
        }

        let terms = terms.into_iter().collect::<Option<Vec<_>>>()?;

        let ty = Self::solve_projection_terms(db, *root, &path, &terms)?;
        Some(ProjectionResult {
            ty,
            projection_evidence: evidence,
        })
    }

    /// Inference-time API: projects a nested value that mentions multiple cycle roots.
    ///
    /// This records the operation result and evidence for result-level cycle recovery, but does
    /// not try to solve any one root-local projection variable immediately.
    fn try_multi_root_nested_cycle_projection_result(
        self,
        db: &'db dyn Db,
        op: ProjectionOp<'db>,
        roots: &[DivergentType],
        mut project: impl FnMut(Self) -> Option<ProjectionTerm<'db>>,
    ) -> Option<ProjectionResult<'db>> {
        if roots.is_empty() {
            return None;
        }

        let root_set = CycleRootSet::from_roots(roots.iter().copied());
        let elements = self.top_level_projection_union_elements(db);
        let mut projection_evidence = ProjectionEvidenceBuilder::default();
        let path = ProjectionPath::from_op(op);
        let mut terms = Vec::with_capacity(elements.len());

        for element in elements {
            let term = if element.mentions_cycle_artifact_in_roots(db, &root_set) {
                // Recursive arms must replay structurally. Re-running the full operation can
                // re-enter projection construction with the same recursive element.
                roots.iter().find_map(|root| {
                    let container = ProjectionContainer::try_from(db, *root, element, None)?;
                    container.project_multi_root_path(db, *root, None, &path)
                })?
            } else {
                project(element)?
            };
            projection_evidence.record_projected_arm(
                db,
                roots.iter().copied(),
                element,
                &path,
                term,
            );
            terms.push(term.ty(db));
        }

        Some(ProjectionResult {
            ty: UnionType::from_elements_cycle_recovery(db, terms),
            projection_evidence: projection_evidence.finish(db),
        })
    }

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
        !self.cycle_artifact_roots(db).is_empty()
    }

    fn has_top_level_cycle_artifact(self, db: &'db dyn Db) -> bool {
        match self {
            Type::Divergent(_) | Type::Projection(_) => true,
            Type::Union(union) => union.elements(db).iter().any(Self::is_cycle_artifact),
            _ => false,
        }
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

/// Inference-time result of a projection, plus facts needed to replay it during recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(crate) struct ProjectionResult<'db> {
    ty: Type<'db>,
    projection_evidence: Option<ProjectionEvidenceSet<'db>>,
}

impl<'db> ProjectionResult<'db> {
    const fn new(ty: Type<'db>) -> Self {
        Self {
            ty,
            projection_evidence: None,
        }
    }

    pub(crate) const fn ty(self) -> Type<'db> {
        self.ty
    }

    pub(crate) const fn projection_evidence(self) -> Option<ProjectionEvidenceSet<'db>> {
        self.projection_evidence
    }
}
