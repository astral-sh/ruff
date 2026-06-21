//! Cycle-recovery projections.
//!
//! Recursive inference can encounter operations on a value whose final type is
//! still being inferred. This module records those operations as projection
//! paths, then solves them once the recovered recursive type exposes enough
//! concrete container structure.

use std::cell::RefCell;

use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use smallvec::SmallVec;
use ty_python_core::EvaluationMode;

use super::{
    DivergentType, DynamicType, KnownClass, MemberLookupPolicy, StaticClassLiteral, TupleSpec,
    Type, UnionBuilder, UnionType, call::CallArguments, instance::SliceLiteral,
    subscript::SubscriptError,
};
use crate::place::{DefinedPlace, Definedness, Place};
use crate::subscript::{PyIndex, PySlice};
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::tuple::{Tuple, TupleLength, TupleType};
use crate::types::visitor::any_over_type;
use crate::{Db, FxIndexMap, FxIndexSet};

/// A type slot participating in result-level projection cycle recovery.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProjectionRecoverySlot<'db> {
    pub(crate) previous: Option<Type<'db>>,
    pub(crate) joined: Type<'db>,
    role: ProjectionRecoverySlotRole,
}

#[derive(Debug, Clone, Copy)]
enum ProjectionRecoverySlotRole {
    DemandOnly,
    Candidate {
        /// The cycle root this result slot represented in the previous iteration, if unique.
        root_hint: Option<DivergentType>,
    },
}

/// Solved projection variables for one Salsa cycle recovery step.
pub(crate) struct ProjectionSolutions<'db> {
    solved: FxIndexMap<ProjectionVar<'db>, Type<'db>>,
}

/// Cycle-recovery-time accumulator for result-wide projection solving.
pub(crate) struct ProjectionRecoveryBuilder<'db> {
    roots: CycleRootSet,
    slots: Vec<ProjectionRecoverySlot<'db>>,
}

impl<'db> ProjectionRecoveryBuilder<'db> {
    pub(crate) fn new(cycle: &salsa::Cycle) -> Self {
        Self {
            roots: CycleRootSet::from_cycle(cycle),
            slots: Vec::new(),
        }
    }

    /// Cycle-recovery-time API: records a joined result slot that can contain projection demands.
    pub(crate) fn push(&mut self, previous: Option<Type<'db>>, joined: Type<'db>) -> Type<'db> {
        self.slots.push(ProjectionRecoverySlot {
            previous,
            joined,
            role: ProjectionRecoverySlotRole::DemandOnly,
        });
        joined
    }

    /// Cycle-recovery-time API: records a joined result slot that can also act as a root candidate.
    pub(crate) fn push_candidate(
        &mut self,
        db: &'db dyn Db,
        previous: Option<Type<'db>>,
        joined: Type<'db>,
    ) -> Type<'db> {
        let root_hint =
            previous.and_then(|previous| root_candidate_from_previous(db, previous, &self.roots));
        self.slots.push(ProjectionRecoverySlot {
            previous,
            joined,
            role: ProjectionRecoverySlotRole::Candidate { root_hint },
        });
        joined
    }

    /// Cycle-recovery-time API: solves all projection variables visible in the recorded slots.
    pub(crate) fn finish(
        &self,
        db: &'db dyn Db,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
    ) -> Option<ProjectionSolutions<'db>> {
        ProjectionSolutions::from_recovery_slots(db, &self.roots, &self.slots, evidence)
    }
}

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

    /// Inference-time API: projects a zero-argument method call.
    pub(crate) fn try_method_call_projection_result(
        self,
        db: &'db dyn Db,
        method_name: &Name,
    ) -> Option<ProjectionResult<'db>> {
        let op = ProjectionOp::CallMethod0(ProjectionMethodName::new(db, method_name));
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
            projection_evidence.record_projected_container_arm(
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
            projection_evidence.record_projected_container_arm(db, [*root], element, &path, term);
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
                roots
                    .iter()
                    .find_map(|root| {
                        let container = ProjectionContainer::try_from(db, *root, element, None)?;
                        container.project_multi_root_path(db, *root, None, &path)
                    })
                    .or_else(|| project(element))?
            } else {
                project(element)?
            };
            projection_evidence.record_projected_container_arm(
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

        Some(Self::union_projection_cycle_recovery(db, elements))
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

    fn has_projection_demands(self, db: &'db dyn Db) -> bool {
        any_over_type(db, self, false, |nested| {
            matches!(nested, Type::Projection(_))
        })
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
            _ => {
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

/// A container shape that can explain projections of a cycle root.
#[derive(Debug, Clone)]
enum ProjectionContainer<'db> {
    Tuple { spec: TupleSpec<'db> },
    Generic(ProjectionContainerFact<'db>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectionReplayMode {
    SingleRoot,
    MultiRoot,
}

impl ProjectionReplayMode {
    const fn projects_cycle_artifacts(self) -> bool {
        matches!(self, Self::MultiRoot)
    }
}

impl<'db> ProjectionContainer<'db> {
    /// Cycle-recovery-time API: builds a container from direct structure or stored evidence.
    fn try_from(
        db: &'db dyn Db,
        root: DivergentType,
        ty: Type<'db>,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
    ) -> Option<Self> {
        if let Some(spec) = ty.exact_tuple_instance_spec(db) {
            return Some(Self::Tuple {
                spec: spec.as_ref().clone(),
            });
        }

        if let Some(fact) = ProjectionContainerFact::try_from_recovery_type(db, ty) {
            return Some(Self::Generic(fact));
        }

        let fact = evidence?.container_fact_for_arm(db, root, ty)?;
        Some(Self::Generic(fact.clone()))
    }

    /// Cycle-recovery-time API: replays all demanded paths against this container.
    fn collect_projection_terms(
        &self,
        db: &'db dyn Db,
        root: DivergentType,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
        terms_by_op: &mut FxIndexMap<ProjectionPath<'db>, Vec<ProjectionTerm<'db>>>,
    ) -> Option<()> {
        for (path, terms) in terms_by_op {
            terms.push(self.project_path(db, root, evidence, path)?);
        }
        Some(())
    }

    /// Cycle-recovery-time API: replays one projection path against this container.
    fn project_path(
        &self,
        db: &'db dyn Db,
        root: DivergentType,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
        path: &ProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        self.project_path_impl(db, root, evidence, path, ProjectionReplayMode::SingleRoot)
    }

    fn project_multi_root_path(
        &self,
        db: &'db dyn Db,
        root: DivergentType,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
        path: &ProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        self.project_path_impl(db, root, evidence, path, ProjectionReplayMode::MultiRoot)
    }

    fn project_path_impl(
        &self,
        db: &'db dyn Db,
        root: DivergentType,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
        path: &ProjectionPath<'db>,
        mode: ProjectionReplayMode,
    ) -> Option<ProjectionTerm<'db>> {
        let ty = match self {
            Self::Tuple { spec } => Type::tuple(TupleType::new(db, spec)),
            Self::Generic(fact) => {
                if let Some(term) = evidence
                    .and_then(|evidence| evidence.project_generic_path(db, root, fact.arm, path))
                {
                    return Some(term);
                }

                return None;
            }
        };
        Self::project_type_path_impl(db, ty, root, evidence, path, mode)
    }

    /// Cycle-recovery-time API: structurally replays a projection path against a type.
    fn project_type_path(
        db: &'db dyn Db,
        ty: Type<'db>,
        root: DivergentType,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
        path: &ProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        Self::project_type_path_impl(
            db,
            ty,
            root,
            evidence,
            path,
            ProjectionReplayMode::SingleRoot,
        )
    }

    fn project_multi_root_type_path(
        db: &'db dyn Db,
        ty: Type<'db>,
        root: DivergentType,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
        path: &ProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        Self::project_type_path_impl(
            db,
            ty,
            root,
            evidence,
            path,
            ProjectionReplayMode::MultiRoot,
        )
    }

    fn project_type_path_impl(
        db: &'db dyn Db,
        ty: Type<'db>,
        root: DivergentType,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
        path: &ProjectionPath<'db>,
        mode: ProjectionReplayMode,
    ) -> Option<ProjectionTerm<'db>> {
        if mode.projects_cycle_artifacts()
            && let Type::Divergent(divergent) = ty
        {
            return Some(ProjectionTerm::Exact(Type::Projection(
                ProjectionType::new(db, divergent, path.clone()),
            )));
        }

        if mode.projects_cycle_artifacts()
            && let Type::Projection(projection) = ty
        {
            return Some(ProjectionTerm::Exact(Type::Projection(
                ProjectionType::new(
                    db,
                    projection.root(db),
                    projection.path(db).append_path(path),
                ),
            )));
        }

        if let Type::Union(union) = ty {
            let terms = union
                .elements(db)
                .iter()
                .map(|element| {
                    Self::project_type_path_impl(db, *element, root, evidence, path, mode)
                })
                .collect::<Option<Vec<_>>>()?;
            return ProjectionTerm::from_union_terms(db, &terms);
        }

        if let Some(term) =
            evidence.and_then(|evidence| evidence.project_generic_path(db, root, ty, path))
        {
            return Some(term);
        }

        let ops = path.ops();
        let (&op, tail) = ops.split_first()?;

        let single_op_path = ProjectionPath::from_op(op);
        let projected = evidence
            .and_then(|evidence| evidence.project_generic_path(db, root, ty, &single_op_path))
            .or_else(|| Self::project_op(db, ty, op))?;

        if tail.is_empty() {
            return Some(projected);
        }

        Self::project_term_path_impl(
            db,
            projected,
            root,
            evidence,
            &ProjectionPath::from_ops(tail.iter().copied()),
            mode,
        )
    }

    fn project_term_path_impl(
        db: &'db dyn Db,
        term: ProjectionTerm<'db>,
        root: DivergentType,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
        path: &ProjectionPath<'db>,
        mode: ProjectionReplayMode,
    ) -> Option<ProjectionTerm<'db>> {
        let ProjectionTerm::List(element) = term else {
            return Self::project_type_path_impl(db, term.ty(db), root, evidence, path, mode);
        };

        // Preserve the list wrapper from starred unpacking while applying the tail path.
        // Converting to `list[T]` would require generic-container evidence for this synthetic list.
        let ops = path.ops();
        let (&op, tail) = ops.split_first()?;
        let projected = Self::project_list_op(db, element, op)?;
        if tail.is_empty() {
            return Some(projected);
        }

        Self::project_term_path_impl(
            db,
            projected,
            root,
            evidence,
            &ProjectionPath::from_ops(tail.iter().copied()),
            mode,
        )
    }

    fn project_list_op(
        db: &'db dyn Db,
        element: Type<'db>,
        op: ProjectionOp<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        match op {
            ProjectionOp::Iter { is_async: false } => Some(ProjectionTerm::Homogeneous(element)),
            ProjectionOp::Iter { is_async: true } => None,
            ProjectionOp::Unpack(UnpackProjection::Exact { .. }) => {
                Some(ProjectionTerm::Homogeneous(element))
            }
            ProjectionOp::Unpack(UnpackProjection::Star { position, .. }) => {
                Some(Self::star_unpack_homogeneous(element, position))
            }
            ProjectionOp::Subscript(
                ProjectionSubscript::Unknown
                | ProjectionSubscript::Int
                | ProjectionSubscript::LiteralInt(_),
            ) => Some(ProjectionTerm::Homogeneous(element)),
            ProjectionOp::Subscript(ProjectionSubscript::StaticSlice(_)) => Some(
                ProjectionTerm::Exact(KnownClass::List.to_specialized_instance(db, &[element])),
            ),
            ProjectionOp::Subscript(ProjectionSubscript::KeyType(_)) => None,
            ProjectionOp::CallMethod0(_)
            | ProjectionOp::ContextEnter { .. }
            | ProjectionOp::AwaitResult => None,
        }
    }

    /// Cycle-recovery-time API: structurally replays one projection operation.
    fn project_op(
        db: &'db dyn Db,
        ty: Type<'db>,
        op: ProjectionOp<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        match op {
            ProjectionOp::Iter { is_async } => Self::project_iter_item(db, ty, is_async),
            ProjectionOp::Unpack(unpack) => Self::project_unpack(db, ty, unpack),
            ProjectionOp::Subscript(subscript) => Self::project_subscript(db, ty, subscript),
            ProjectionOp::CallMethod0(_)
            | ProjectionOp::ContextEnter { .. }
            | ProjectionOp::AwaitResult => None,
        }
    }

    fn project_star_unpack_tuple(
        db: &'db dyn Db,
        tuple: &TupleSpec<'db>,
        prefix: usize,
        suffix: usize,
        position: StarUnpackPosition,
    ) -> Option<ProjectionTerm<'db>> {
        let resized = tuple
            .resize(db, TupleLength::Variable(prefix, suffix))
            .ok()?;
        let Tuple::Variable(tuple) = resized else {
            return None;
        };

        Some(match position {
            StarUnpackPosition::Prefix(index) => {
                ProjectionTerm::Exact(tuple.iter_prefix_elements().nth(index)?)
            }
            StarUnpackPosition::Rest => ProjectionTerm::List(tuple.variable()),
            StarUnpackPosition::Suffix(index) => {
                ProjectionTerm::Exact(tuple.iter_suffix_elements().nth(index)?)
            }
        })
    }

    fn project_iter_item(
        db: &'db dyn Db,
        ty: Type<'db>,
        is_async: bool,
    ) -> Option<ProjectionTerm<'db>> {
        if let Some(spec) = ty.exact_tuple_instance_spec(db) {
            if is_async {
                return None;
            }

            return Some(ProjectionTerm::Homogeneous(
                spec.as_ref().homogeneous_element_type(db),
            ));
        }

        None
    }

    fn project_unpack(
        db: &'db dyn Db,
        ty: Type<'db>,
        unpack: UnpackProjection,
    ) -> Option<ProjectionTerm<'db>> {
        match unpack {
            UnpackProjection::Exact { len, index } => {
                Self::project_unpack_exact(db, ty, len, index)
            }
            UnpackProjection::Star {
                prefix,
                suffix,
                position,
            } => Self::project_star_unpack(db, ty, prefix, suffix, position),
        }
    }

    fn project_unpack_exact(
        db: &'db dyn Db,
        ty: Type<'db>,
        len: usize,
        index: usize,
    ) -> Option<ProjectionTerm<'db>> {
        if let Some(spec) = ty.exact_tuple_instance_spec(db) {
            let tuple = spec.as_ref().resize(db, TupleLength::Fixed(len)).ok()?;
            let Tuple::Fixed(tuple) = tuple else {
                return None;
            };
            return Some(ProjectionTerm::Exact(tuple.iter_all_elements().nth(index)?));
        }

        None
    }

    fn project_star_unpack(
        db: &'db dyn Db,
        ty: Type<'db>,
        prefix: usize,
        suffix: usize,
        position: StarUnpackPosition,
    ) -> Option<ProjectionTerm<'db>> {
        if let Some(spec) = ty.exact_tuple_instance_spec(db) {
            return Self::project_star_unpack_tuple(db, spec.as_ref(), prefix, suffix, position);
        }

        None
    }

    const fn star_unpack_homogeneous(
        element: Type<'db>,
        position: StarUnpackPosition,
    ) -> ProjectionTerm<'db> {
        match position {
            StarUnpackPosition::Prefix(_) | StarUnpackPosition::Suffix(_) => {
                ProjectionTerm::Homogeneous(element)
            }
            StarUnpackPosition::Rest => ProjectionTerm::List(element),
        }
    }

    fn project_subscript(
        db: &'db dyn Db,
        ty: Type<'db>,
        subscript: ProjectionSubscript<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        if let Some(spec) = ty.exact_tuple_instance_spec(db) {
            let tuple = spec.as_ref();

            return match subscript {
                ProjectionSubscript::LiteralInt(index) => {
                    let index = i32::try_from(index).ok()?;
                    Some(ProjectionTerm::Exact(tuple.py_index(db, index).ok()?))
                }
                ProjectionSubscript::Int | ProjectionSubscript::Unknown => Some(
                    ProjectionTerm::Homogeneous(tuple.homogeneous_element_type(db)),
                ),
                ProjectionSubscript::KeyType(_) => None,
                ProjectionSubscript::StaticSlice(slice) => match tuple {
                    TupleSpec::Fixed(tuple) => {
                        let elements = tuple
                            .py_slice(db, slice.start, slice.stop, slice.step)
                            .ok()?;
                        Some(ProjectionTerm::Exact(Type::heterogeneous_tuple(
                            db, elements,
                        )))
                    }
                    TupleSpec::Variable(tuple) => {
                        let element = UnionType::from_elements_leave_aliases(
                            db,
                            tuple
                                .iter_prefix_elements()
                                .chain(std::iter::once(tuple.variable()))
                                .chain(tuple.iter_suffix_elements()),
                        );
                        Some(ProjectionTerm::Exact(Type::homogeneous_tuple(db, element)))
                    }
                },
            };
        }

        None
    }

    fn into_type(
        self,
        db: &'db dyn Db,
        root: DivergentType,
        solved_ops: &FxIndexMap<ProjectionPath<'db>, Type<'db>>,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
    ) -> Option<Type<'db>> {
        match self {
            Self::Tuple { spec } => match spec {
                Tuple::Fixed(tuple) => {
                    let elements = tuple
                        .iter_all_elements()
                        .map(|element| {
                            element
                                .replace_solved_projection_artifacts(db, root, solved_ops, evidence)
                        })
                        .collect::<Option<Vec<_>>>()?;

                    Some(Type::heterogeneous_tuple(db, elements))
                }
                Tuple::Variable(tuple) => {
                    let prefix = tuple
                        .iter_prefix_elements()
                        .map(|element| {
                            element
                                .replace_solved_projection_artifacts(db, root, solved_ops, evidence)
                        })
                        .collect::<Option<Vec<_>>>()?;
                    let variable = tuple
                        .variable()
                        .replace_solved_projection_artifacts(db, root, solved_ops, evidence)?;
                    let suffix = tuple
                        .iter_suffix_elements()
                        .map(|element| {
                            element
                                .replace_solved_projection_artifacts(db, root, solved_ops, evidence)
                        })
                        .collect::<Option<Vec<_>>>()?;

                    Some(Type::tuple(TupleType::mixed(db, prefix, variable, suffix)))
                }
            },
            Self::Generic(fact) => {
                let arguments = fact
                    .arguments
                    .iter()
                    .map(|argument| {
                        (*argument)
                            .replace_solved_projection_artifacts(db, root, solved_ops, evidence)
                    })
                    .collect::<Option<Vec<_>>>()?;

                Type::from(fact.class.apply_specialization(db, |generic_context| {
                    generic_context.specialize(db, arguments)
                }))
                .to_instance(db)
            }
        }
    }

    fn infer_method_call0_type_for_type(
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

    fn infer_projection_path(
        db: &'db dyn Db,
        ty: Type<'db>,
        path: &ProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        if let Type::Union(union) = ty {
            let terms = union
                .elements(db)
                .iter()
                .map(|element| Self::infer_projection_path(db, *element, path))
                .collect::<Option<Vec<_>>>()?;
            return ProjectionTerm::from_union_terms(db, &terms);
        }

        let ops = path.ops();
        let (&op, tail) = ops.split_first()?;

        let projected = Self::infer_projection_op(db, ty, op)?;
        if tail.is_empty() {
            return Some(projected);
        }

        Self::infer_projection_path(
            db,
            projected.ty(db),
            &ProjectionPath::from_ops(tail.iter().copied()),
        )
    }

    fn infer_projection_op(
        db: &'db dyn Db,
        ty: Type<'db>,
        op: ProjectionOp<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        match op {
            ProjectionOp::Iter { is_async } => {
                let mode = EvaluationMode::from_is_async(is_async);
                Some(ProjectionTerm::Homogeneous(
                    ty.try_iterate_with_mode(db, mode)
                        .ok()?
                        .homogeneous_element_type(db),
                ))
            }
            ProjectionOp::Unpack(unpack) => Self::infer_unpack(db, ty, unpack),
            ProjectionOp::Subscript(subscript) => Some(ProjectionTerm::Exact(
                ty.subscript_without_projection(db, subscript.to_type(db), ast::ExprContext::Load)
                    .ok()?,
            )),
            ProjectionOp::CallMethod0(method) => Some(ProjectionTerm::Exact(
                Self::infer_method_call0_type_for_type(db, ty, method.name(db))?,
            )),
            ProjectionOp::ContextEnter { is_async } => {
                let mode = EvaluationMode::from_is_async(is_async);
                Some(ProjectionTerm::Exact(
                    ty.try_enter_with_mode(db, mode).ok()?,
                ))
            }
            ProjectionOp::AwaitResult => Some(ProjectionTerm::Exact(ty.try_await(db).ok()?)),
        }
    }

    fn infer_unpack(
        db: &'db dyn Db,
        ty: Type<'db>,
        unpack: UnpackProjection,
    ) -> Option<ProjectionTerm<'db>> {
        match unpack {
            UnpackProjection::Exact { len, index } => Self::infer_unpack_exact(db, ty, len, index),
            UnpackProjection::Star {
                prefix,
                suffix,
                position,
            } => Self::infer_star_unpack(db, ty, prefix, suffix, position),
        }
    }

    fn infer_unpack_exact(
        db: &'db dyn Db,
        ty: Type<'db>,
        len: usize,
        index: usize,
    ) -> Option<ProjectionTerm<'db>> {
        if let Some(spec) = ty.exact_tuple_instance_spec(db) {
            let tuple = spec.as_ref().resize(db, TupleLength::Fixed(len)).ok()?;
            let Tuple::Fixed(tuple) = tuple else {
                return None;
            };
            return Some(ProjectionTerm::Exact(tuple.iter_all_elements().nth(index)?));
        }

        Some(ProjectionTerm::Homogeneous(
            ty.try_iterate(db).ok()?.homogeneous_element_type(db),
        ))
    }

    fn infer_star_unpack(
        db: &'db dyn Db,
        ty: Type<'db>,
        prefix: usize,
        suffix: usize,
        position: StarUnpackPosition,
    ) -> Option<ProjectionTerm<'db>> {
        if let Some(spec) = ty.exact_tuple_instance_spec(db) {
            return Self::project_star_unpack_tuple(db, spec.as_ref(), prefix, suffix, position);
        }

        let element = ty.try_iterate(db).ok()?.homogeneous_element_type(db);
        Some(Self::star_unpack_homogeneous(element, position))
    }
}

/// Cycle-recovery-time equation system for projection variables.
///
/// Each [`ProjectionPath`] is a variable `A_p = Projection_p(root)`. Equations contain only
/// query-free facts collected from container arms: closed productive terms and flat dependencies on
/// other projection variables. Constructor-guarded recursion is filled with `Divergent`.
///
/// The system is solved only for the paths collected from the candidate type. If replay exposes a
/// dependency on a path outside that set, solving fails closed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
struct ProjectionVar<'db> {
    root: DivergentType,
    path: ProjectionPath<'db>,
}

#[derive(Debug, Clone)]
struct CycleRootSet {
    roots: SmallVec<[DivergentType; 4]>,
}

impl CycleRootSet {
    fn from_cycle(cycle: &salsa::Cycle) -> Self {
        Self {
            roots: cycle.head_ids().map(DivergentType::new).collect(),
        }
    }

    fn single(root: DivergentType) -> Self {
        Self {
            roots: SmallVec::from_slice(&[root]),
        }
    }

    fn from_roots(roots: impl IntoIterator<Item = DivergentType>) -> Self {
        Self {
            roots: roots.into_iter().collect(),
        }
    }

    fn len(&self) -> usize {
        self.roots.len()
    }

    fn contains(&self, root: DivergentType) -> bool {
        self.roots
            .iter()
            .any(|candidate| candidate.same_marker(root))
    }
}

impl<'db> ProjectionSolutions<'db> {
    fn from_recovery_slots(
        db: &'db dyn Db,
        roots: &CycleRootSet,
        slots: &[ProjectionRecoverySlot<'db>],
        evidence: Option<&ProjectionEvidenceSet<'db>>,
    ) -> Option<Self> {
        ProjectionEquationSystem::from_recovery_slots(db, roots, slots, evidence)?.solve(db)
    }

    fn new(solved: FxIndexMap<ProjectionVar<'db>, Type<'db>>) -> Self {
        Self { solved }
    }

    fn roots(&self) -> CycleRootSet {
        let mut roots = SmallVec::new();
        for var in self.solved.keys() {
            if !roots
                .iter()
                .any(|candidate: &DivergentType| candidate.same_marker(var.root))
            {
                roots.push(var.root);
            }
        }
        CycleRootSet { roots }
    }

    fn solved_type(&self, db: &'db dyn Db, var: &ProjectionVar<'db>) -> Option<Type<'db>> {
        if let Some(solved) = self.solved.get(var).copied() {
            return Some(solved);
        }

        // Longer demanded paths are represented by the nearest solved prefix plus a
        // structural replay of the remaining operations.
        for prefix_len in (1..var.path.ops().len()).rev() {
            let prefix = ProjectionPath::from_ops(var.path.ops()[..prefix_len].iter().copied());
            let Some(solved) = self.solved.get(&ProjectionVar {
                root: var.root,
                path: prefix,
            }) else {
                continue;
            };
            let tail = ProjectionPath::from_ops(var.path.ops()[prefix_len..].iter().copied());
            if solved.same_divergent_marker(db, Type::Divergent(var.root)) {
                return Some(*solved);
            }
            return ProjectionContainer::project_type_path(db, *solved, var.root, None, &tail)
                .map(|term| term.ty(db));
        }

        None
    }
}

struct ProjectionEquationSystem<'db> {
    equations: FxIndexMap<ProjectionVar<'db>, ProjectionEquation<'db>>,
}

impl<'db> ProjectionEquationSystem<'db> {
    fn from_recovery_slots(
        db: &'db dyn Db,
        roots: &CycleRootSet,
        slots: &[ProjectionRecoverySlot<'db>],
        evidence: Option<&ProjectionEvidenceSet<'db>>,
    ) -> Option<Self> {
        if roots.len() <= 1 {
            return None;
        }

        let mut demands = FxIndexSet::default();
        for slot in slots {
            for (root, path) in slot.joined.projection_demands(db) {
                if roots.contains(root) {
                    demands.insert(ProjectionVar { root, path });
                }
            }
        }

        if demands.is_empty() {
            return None;
        }

        let mut candidates = RootCandidates::default();
        for slot in slots {
            let ProjectionRecoverySlotRole::Candidate { root_hint } = slot.role else {
                continue;
            };
            let joined_demands = slot.joined.projection_demands(db);
            if let Some(root) =
                root_hint.or_else(|| root_candidate_from_demands(&joined_demands, roots))
            {
                if !demands.iter().any(|var| var.root.same_marker(root)) {
                    continue;
                }
                if !is_plausible_root_candidate(db, root, slot.joined, evidence) {
                    continue;
                }
                candidates.insert(root, slot.joined);
                continue;
            }

            if slot.previous.is_none() {
                for (root, _) in joined_demands {
                    if demands.iter().any(|var| var.root.same_marker(root))
                        && is_plausible_root_candidate(db, root, slot.joined, evidence)
                    {
                        candidates.insert(root, slot.joined);
                    }
                }
            }
        }

        let demands = demands.into_iter().collect::<Vec<_>>();
        let mut equations = FxIndexMap::default();
        // A longer path whose prefix is also demanded is solved by replaying the tail
        // on the prefix solution. Keeping both as independent variables can build an
        // infinite chain such as `A_[0]^n -> B_[0]^(2n)`.
        let mut pending = demands
            .iter()
            .filter(|var| {
                !demands.iter().any(|prefix| {
                    prefix.root.same_marker(var.root) && prefix.path.is_strict_prefix_of(&var.path)
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        while let Some(var) = pending.pop() {
            if equations.contains_key(&var) {
                continue;
            }

            let mut equation = ProjectionEquation::default();
            let mut has_equation_terms = false;
            if let Some(candidates) = candidates.get(var.root) {
                for candidate in candidates {
                    let Some(candidate_equation) =
                        Self::build_equation(db, roots, *candidate, &var, evidence)
                    else {
                        continue;
                    };
                    has_equation_terms = true;
                    equation.merge(candidate_equation)?;
                }
            }
            if !has_equation_terms && let Some(evidence) = evidence {
                for fact in evidence.projection_facts(db) {
                    if fact.root.same_marker(var.root) && fact.path == var.path {
                        has_equation_terms = true;
                        equation.add_projection_term(db, roots, &var, fact.term, true)?;
                    }
                }
            }
            if !has_equation_terms {
                return None;
            }
            equation.wrap_in_list?;
            if equation.unsupported {
                return None;
            }
            for dependency in &equation.dependencies {
                if !equations.contains_key(dependency) {
                    pending.push(dependency.clone());
                }
            }
            equations.insert(var, equation);
        }

        Some(Self { equations })
    }

    fn from_terms_by_op(
        db: &'db dyn Db,
        root: DivergentType,
        terms_by_op: &FxIndexMap<ProjectionPath<'db>, Vec<ProjectionTerm<'db>>>,
    ) -> Option<Self> {
        let roots = CycleRootSet::single(root);
        let mut equations = FxIndexMap::default();
        let mut pending = terms_by_op
            .keys()
            .map(|path| ProjectionVar {
                root,
                path: path.clone(),
            })
            .collect::<Vec<_>>();

        while let Some(var) = pending.pop() {
            if equations.contains_key(&var) {
                continue;
            }

            let terms = terms_by_op.get(&var.path)?;
            let mut equation = ProjectionEquation::default();
            for term in terms {
                equation.add_projection_term(db, &roots, &var, *term, true)?;
            }
            equation.wrap_in_list?;
            if equation.unsupported {
                return None;
            }
            for dependency in &equation.dependencies {
                if !equations.contains_key(dependency) {
                    pending.push(dependency.clone());
                }
            }
            equations.insert(var, equation);
        }

        Some(Self { equations })
    }

    fn build_equation(
        db: &'db dyn Db,
        roots: &CycleRootSet,
        candidate: Type<'db>,
        var: &ProjectionVar<'db>,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
    ) -> Option<ProjectionEquation<'db>> {
        let elements = candidate.top_level_projection_union_elements(db);
        let mut equation = ProjectionEquation::default();

        for element in elements {
            if element.same_divergent_marker(db, Type::Divergent(var.root)) {
                continue;
            }

            let container = ProjectionContainer::try_from(db, var.root, element, evidence)?;
            let term = container.project_multi_root_path(db, var.root, evidence, &var.path)?;
            // Terms projected from an arm that already contains this root are recursive evidence,
            // not a productive base. Cross-root projection terms may still be productive because
            // they can be substituted once the other root is solved.
            let allow_productive = !element.mentions_cycle_artifact_direct(db, var.root);
            equation.add_projection_term(db, roots, var, term, allow_productive)?;
        }

        Some(equation)
    }

    fn solve(self, db: &'db dyn Db) -> Option<ProjectionSolutions<'db>> {
        let Self { equations } = self;
        if equations.is_empty() || equations.values().any(|equation| equation.unsupported) {
            return None;
        }

        let vars = equations.keys().cloned().collect::<Vec<_>>();
        let var_indices = vars
            .iter()
            .enumerate()
            .map(|(index, var)| (var.clone(), index))
            .collect::<FxIndexMap<_, _>>();

        let mut graph = vec![Vec::new(); vars.len()];
        for (var, equation) in &equations {
            let source = var_indices[var];
            for dependency in &equation.dependencies {
                graph[source].push(*var_indices.get(dependency)?);
            }
        }

        let sccs = dependency_first_strongly_connected_components(&graph);
        let mut solutions = vec![None; vars.len()];
        for scc in sccs {
            let wrap_in_list = equations[&vars[*scc.first()?]].wrap_in_list?;
            for &index in &scc {
                if equations[&vars[index]].wrap_in_list != Some(wrap_in_list) {
                    return None;
                }
            }

            let scc_vars = scc
                .iter()
                .map(|&index| vars[index].clone())
                .collect::<FxIndexSet<_>>();
            let scc_is_divergent = scc.iter().any(|&index| {
                let equation = &equations[&vars[index]];
                equation.divergent
                    || equation
                        .productive
                        .iter()
                        .any(|term| term.mentions_projection_var_in(db, &scc_vars))
            });
            let solved_so_far = vars
                .iter()
                .cloned()
                .zip(solutions.iter().copied())
                .filter_map(|(var, solution)| Some((var, solution?)))
                .collect::<FxIndexMap<_, _>>();
            let solved_so_far = ProjectionSolutions::new(solved_so_far);

            if scc_is_divergent {
                for &index in &scc {
                    let mut base = Vec::new();
                    let equation = &equations[&vars[index]];
                    for term in &equation.productive {
                        if term.mentions_projection_var_in(db, &scc_vars) {
                            continue;
                        }
                        base.push(
                            term.replace_solved_projection_vars(db, &solved_so_far)
                                .unwrap_or(*term),
                        );
                    }
                    for dependency in &equation.dependencies {
                        let dependency_index = var_indices[dependency];
                        if !scc.contains(&dependency_index) {
                            base.push(solutions[dependency_index]?);
                        }
                    }
                    let root = vars[index].root;
                    base.push(Type::Divergent(root));
                    let solved = match base.as_slice() {
                        [term] => *term,
                        _ => UnionType::from_elements_cycle_recovery(db, base),
                    };
                    solutions[index] = Some(if wrap_in_list {
                        KnownClass::List.to_specialized_instance(db, &[solved])
                    } else {
                        solved
                    });
                }
                continue;
            }

            let mut base = Vec::new();
            for &index in &scc {
                let equation = &equations[&vars[index]];
                for term in &equation.productive {
                    base.push(
                        term.replace_solved_projection_vars(db, &solved_so_far)
                            .unwrap_or(*term),
                    );
                }
                for dependency in &equation.dependencies {
                    let dependency_index = var_indices[dependency];
                    if !scc.contains(&dependency_index) {
                        base.push(solutions[dependency_index]?);
                    }
                }
            }

            if base.is_empty() {
                for index in scc {
                    let root = vars[index].root;
                    let solved = if wrap_in_list {
                        KnownClass::List.to_specialized_instance(db, &[Type::Divergent(root)])
                    } else {
                        Type::Divergent(root)
                    };
                    solutions[index] = Some(solved);
                }
                continue;
            }

            let solved = match base.as_slice() {
                [term] => *term,
                _ => UnionType::from_elements_cycle_recovery(db, base),
            };
            let solved = if wrap_in_list {
                KnownClass::List.to_specialized_instance(db, &[solved])
            } else {
                solved
            };
            for index in scc {
                solutions[index] = Some(solved);
            }
        }

        let solved = vars
            .into_iter()
            .enumerate()
            .map(|(index, var)| Some((var, solutions[index]?)))
            .collect::<Option<FxIndexMap<_, _>>>()?;

        Some(ProjectionSolutions::new(solved))
    }
}

#[derive(Default)]
struct RootCandidates<'db> {
    candidates: FxIndexMap<DivergentType, Vec<Type<'db>>>,
}

impl<'db> RootCandidates<'db> {
    fn insert(&mut self, root: DivergentType, ty: Type<'db>) {
        if let Some((_, candidates)) = self
            .candidates
            .iter_mut()
            .find(|(candidate, _)| candidate.same_marker(root))
        {
            if !candidates.contains(&ty) {
                candidates.push(ty);
            }
        } else {
            self.candidates.insert(root, vec![ty]);
        }
    }

    fn get(&self, root: DivergentType) -> Option<&[Type<'db>]> {
        self.candidates
            .iter()
            .find_map(|(candidate, ty)| candidate.same_marker(root).then_some(ty.as_slice()))
    }
}

#[derive(Default)]
struct ProjectionEquation<'db> {
    // Productive terms may still contain projection variables. Variables outside the SCC are
    // substituted with solved values; variables inside the SCC force divergent widening.
    productive: Vec<Type<'db>>,
    dependencies: FxIndexSet<ProjectionVar<'db>>,
    divergent: bool,
    unsupported: bool,
    wrap_in_list: Option<bool>,
}

impl<'db> ProjectionEquation<'db> {
    fn merge(&mut self, other: Self) -> Option<()> {
        match (self.wrap_in_list, other.wrap_in_list) {
            (Some(left), Some(right)) if left != right => return None,
            (None, Some(right)) => self.wrap_in_list = Some(right),
            _ => {}
        }

        self.productive.extend(other.productive);
        self.dependencies.extend(other.dependencies);
        self.divergent |= other.divergent;
        self.unsupported |= other.unsupported;
        Some(())
    }

    fn add_projection_term(
        &mut self,
        db: &'db dyn Db,
        roots: &CycleRootSet,
        var: &ProjectionVar<'db>,
        term: ProjectionTerm<'db>,
        allow_productive: bool,
    ) -> Option<()> {
        let wrap_in_list = matches!(term, ProjectionTerm::List(_));
        match self.wrap_in_list {
            Some(existing) if existing != wrap_in_list => return None,
            None => self.wrap_in_list = Some(wrap_in_list),
            Some(_) => {}
        }

        match term {
            ProjectionTerm::Exact(term) => {
                if term.same_divergent_marker(db, Type::Divergent(var.root)) {
                    self.dependencies.insert(var.clone());
                    return Some(());
                }

                if !term.is_matching_projection(db, var.root, &var.path)
                    && term.mentions_matching_projection(db, var.root, &var.path)
                    && let Some(projected) = ProjectionContainer::project_multi_root_type_path(
                        db, term, var.root, None, &var.path,
                    )
                {
                    if projected
                        .ty(db)
                        .is_matching_projection(db, var.root, &var.path)
                    {
                        self.divergent = true;
                        return Some(());
                    }
                    return self.add_projection_term(db, roots, var, projected, allow_productive);
                }

                self.add_type_term(db, roots, var, term, true, allow_productive)
            }
            ProjectionTerm::Homogeneous(term) => {
                if term.same_divergent_marker(db, Type::Divergent(var.root)) {
                    self.dependencies.insert(var.clone());
                    return Some(());
                }

                self.add_type_term(db, roots, var, term, true, allow_productive)
            }
            ProjectionTerm::List(term) => {
                self.add_type_term(db, roots, var, term, false, allow_productive)
            }
        }
    }

    fn add_type_term(
        &mut self,
        db: &'db dyn Db,
        roots: &CycleRootSet,
        var: &ProjectionVar<'db>,
        term: Type<'db>,
        allow_dependencies: bool,
        allow_productive: bool,
    ) -> Option<()> {
        if let Type::Union(union) = term {
            for element in union.elements(db) {
                self.add_type_term(
                    db,
                    roots,
                    var,
                    *element,
                    allow_dependencies,
                    allow_productive,
                )?;
            }
            return Some(());
        }

        if allow_dependencies {
            if let Type::Projection(projection) = term {
                let root = projection.root(db);
                if roots.contains(root) {
                    let dependency = ProjectionVar {
                        root,
                        path: projection.path(db),
                    };
                    if var.path.is_strict_prefix_of(&dependency.path) {
                        // A strict extension of the current path cannot be closed by
                        // adding another projection variable; widen this equation.
                        self.divergent = true;
                    } else {
                        self.dependencies.insert(dependency);
                    }
                } else {
                    self.unsupported = true;
                }
                return Some(());
            }

            if let Some(var) = term.matching_projection_narrowing_var_multi(db, roots) {
                self.dependencies.insert(var);
                return Some(());
            }
        }

        if term.mentions_cycle_artifact_outside_roots(db, roots) {
            self.unsupported = true;
            return Some(());
        }

        if allow_dependencies {
            let dependencies = term
                .projection_demands(db)
                .into_iter()
                .filter_map(|(root, path)| {
                    roots.contains(root).then_some(ProjectionVar { root, path })
                })
                .collect::<Vec<_>>();
            if !dependencies.is_empty() {
                if dependencies
                    .iter()
                    .any(|dependency| var.path.is_strict_prefix_of(&dependency.path))
                {
                    // A strict extension of the current path cannot be closed by
                    // adding another projection variable; widen this equation.
                    self.divergent = true;
                    return Some(());
                }
                self.dependencies.extend(dependencies);
                if allow_productive {
                    self.productive.push(term);
                }
                return Some(());
            }
        }

        if term.mentions_divergent_in_roots(db, roots)
            || term.mentions_cycle_artifact_in_roots(db, roots)
        {
            self.divergent = true;
            return Some(());
        }

        if allow_productive {
            self.productive.push(term);
        }
        Some(())
    }
}

fn root_candidate_from_previous(
    db: &dyn Db,
    previous: Type<'_>,
    roots: &CycleRootSet,
) -> Option<DivergentType> {
    let mut candidates = previous
        .cycle_artifact_roots(db)
        .into_iter()
        .filter(|root| roots.contains(*root))
        .collect::<Vec<_>>();
    candidates.dedup_by(|left, right| left.same_marker(*right));
    match candidates.as_slice() {
        [root] => Some(*root),
        _ => None,
    }
}

fn root_candidate_from_demands(
    demands: &[(DivergentType, ProjectionPath<'_>)],
    roots: &CycleRootSet,
) -> Option<DivergentType> {
    let mut candidates = Vec::new();
    for (root, _) in demands {
        if roots.contains(*root) {
            Type::push_cycle_artifact_root(&mut candidates, *root);
        }
    }
    match candidates.as_slice() {
        [root] => Some(*root),
        _ => None,
    }
}

fn is_plausible_root_candidate<'db>(
    db: &'db dyn Db,
    root: DivergentType,
    ty: Type<'db>,
    evidence: Option<&ProjectionEvidenceSet<'db>>,
) -> bool {
    ty.top_level_projection_union_elements(db)
        .into_iter()
        .filter(|element| !element.same_divergent_marker(db, Type::Divergent(root)))
        .any(|element| ProjectionContainer::try_from(db, root, element, evidence).is_some())
}

/// Returns strongly connected components (SCC) in dependency-first order for a graph whose edges point
/// from a projection variable to the variables it depends on.
fn dependency_first_strongly_connected_components(graph: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct SccState<'a> {
        graph: &'a [Vec<usize>],
        next_index: usize,
        indices: Vec<Option<usize>>,
        lowlinks: Vec<usize>,
        stack: Vec<usize>,
        on_stack: Vec<bool>,
        components: Vec<Vec<usize>>,
    }

    impl SccState<'_> {
        fn connect(&mut self, node: usize) {
            let index = self.next_index;
            self.next_index += 1;
            self.indices[node] = Some(index);
            self.lowlinks[node] = index;
            self.stack.push(node);
            self.on_stack[node] = true;

            for &dependency in &self.graph[node] {
                if self.indices[dependency].is_none() {
                    self.connect(dependency);
                    self.lowlinks[node] = self.lowlinks[node].min(self.lowlinks[dependency]);
                } else if self.on_stack[dependency]
                    && let Some(dependency_index) = self.indices[dependency]
                {
                    self.lowlinks[node] = self.lowlinks[node].min(dependency_index);
                }
            }

            let Some(node_index) = self.indices[node] else {
                return;
            };

            if self.lowlinks[node] == node_index {
                let mut component = Vec::new();
                while let Some(dependency) = self.stack.pop() {
                    self.on_stack[dependency] = false;
                    component.push(dependency);
                    if dependency == node {
                        break;
                    }
                }
                self.components.push(component);
            }
        }
    }

    let mut state = SccState {
        graph,
        next_index: 0,
        indices: vec![None; graph.len()],
        lowlinks: vec![0; graph.len()],
        stack: Vec::new(),
        on_stack: vec![false; graph.len()],
        components: Vec::new(),
    };

    for node in 0..graph.len() {
        if state.indices[node].is_none() {
            state.connect(node);
        }
    }

    state.components
}

/// The result of applying one projection path to one container arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
enum ProjectionTerm<'db> {
    Exact(Type<'db>),
    Homogeneous(Type<'db>),
    List(Type<'db>),
}

impl<'db> ProjectionTerm<'db> {
    fn ty(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            ProjectionTerm::Exact(ty) | ProjectionTerm::Homogeneous(ty) => ty,
            ProjectionTerm::List(element) => {
                KnownClass::List.to_specialized_instance(db, &[element])
            }
        }
    }

    fn from_union_terms(db: &'db dyn Db, terms: &[Self]) -> Option<Self> {
        let wrap_in_list = terms
            .iter()
            .any(|term| matches!(term, ProjectionTerm::List(_)));
        if wrap_in_list
            && terms
                .iter()
                .any(|term| !matches!(term, ProjectionTerm::List(_)))
        {
            return None;
        }

        let elements = terms.iter().map(|term| match *term {
            ProjectionTerm::List(element) => element,
            ProjectionTerm::Exact(ty) | ProjectionTerm::Homogeneous(ty) => ty,
        });
        let ty = UnionType::from_elements_cycle_recovery(db, elements);
        Some(if wrap_in_list {
            ProjectionTerm::List(ty)
        } else {
            ProjectionTerm::Exact(ty)
        })
    }

    fn is_ambiguous(self, db: &'db dyn Db) -> bool {
        any_over_type(db, self.ty(db), false, |ty| {
            matches!(ty, Type::Dynamic(DynamicType::AmbiguousOverload))
        })
    }
}

/// Projection facts computed during normal inference and reused during cycle recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub(crate) struct ProjectionEvidenceSet<'db>(ProjectionEvidenceSetInterned<'db>);

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ProjectionEvidenceSet<'_> {}

/// Mutable inference-time accumulator for projection evidence.
#[derive(Debug, Clone, Default)]
struct ProjectionEvidenceBuilder<'db> {
    projection_facts: FxIndexSet<ProjectionEvidenceFact<'db>>,
    container_facts: FxIndexSet<ProjectionContainerFact<'db>>,
}

impl<'db> ProjectionEvidenceBuilder<'db> {
    /// Inference-time API: records facts needed by projection cycle recovery.
    fn extend_from_types(&mut self, db: &'db dyn Db, types: impl IntoIterator<Item = Type<'db>>) {
        for ty in types {
            let demands = ty.projection_demands(db);
            if demands.is_empty() {
                continue;
            }

            let generic_containers = ProjectionEvidenceSet::generic_containers(db, ty);
            for container_fact in generic_containers {
                let arm = container_fact.arm;
                let mut has_projection_fact = false;
                for (root, path) in &demands {
                    if arm.same_divergent_marker(db, Type::Divergent(*root)) {
                        continue;
                    }

                    for suffix in path.suffixes() {
                        let Some(term) =
                            ProjectionContainer::infer_projection_path(db, arm, &suffix)
                        else {
                            continue;
                        };
                        if term.is_ambiguous(db) {
                            continue;
                        }
                        has_projection_fact = true;
                        self.push_projection_fact(ProjectionEvidenceFact {
                            root: *root,
                            arm,
                            path: suffix,
                            term,
                        });
                    }
                }
                if has_projection_fact {
                    self.push_container_fact(container_fact);
                }
            }
        }
    }

    /// Inference-time API: records the observed result of projecting a non-cycle arm.
    fn record_projected_container_arm(
        &mut self,
        db: &'db dyn Db,
        roots: impl IntoIterator<Item = DivergentType>,
        arm: Type<'db>,
        path: &ProjectionPath<'db>,
        term: ProjectionTerm<'db>,
    ) {
        if term.is_ambiguous(db) {
            return;
        }

        let Some(container_fact) = ProjectionContainerFact::try_from_inference_type(db, arm) else {
            return;
        };

        self.push_container_fact(container_fact);
        for root in roots {
            self.push_projection_fact(ProjectionEvidenceFact {
                root,
                arm,
                path: path.clone(),
                term,
            });
        }
    }

    fn push_projection_fact(&mut self, fact: ProjectionEvidenceFact<'db>) {
        self.projection_facts.insert(fact);
    }

    fn push_container_fact(&mut self, fact: ProjectionContainerFact<'db>) {
        self.container_facts.insert(fact);
    }

    fn finish(self, db: &'db dyn Db) -> Option<ProjectionEvidenceSet<'db>> {
        ProjectionEvidenceSet::new(db, self.projection_facts, self.container_facts)
    }
}

impl<'db> ProjectionEvidenceSet<'db> {
    /// Inference-time API: eagerly collects projection evidence for later cycle recovery.
    ///
    /// Use this when the projection demand can be introduced after the inference result is
    /// produced, so the result cannot know ahead of time whether evidence will be needed.
    pub(crate) fn from_types(
        db: &'db dyn Db,
        types: impl IntoIterator<Item = Type<'db>>,
    ) -> Option<Self> {
        let mut builder = ProjectionEvidenceBuilder::default();
        builder.extend_from_types(db, types);
        builder.finish(db)
    }

    /// Inference-time API: conditionally collects projection evidence.
    ///
    /// Use this only when every projection demand that may need facts from these types has already
    /// been observed before the inference result is produced. The collection still runs if the
    /// produced types already contain projection demands; `should_collect` controls only demands
    /// that may be introduced later by an external consumer. If an external consumer can later
    /// introduce a new demand for the produced result, use [`ProjectionEvidenceSet::from_types`]
    /// instead.
    pub(crate) fn from_types_if_needed(
        db: &'db dyn Db,
        should_collect: bool,
        types: impl IntoIterator<Item = Type<'db>>,
    ) -> Option<Self> {
        let types = types.into_iter().collect::<SmallVec<[_; 8]>>();
        (should_collect || types.iter().any(|ty| ty.has_projection_demands(db)))
            .then(|| Self::from_types(db, types))
            .flatten()
    }

    pub(crate) fn merged(
        db: &'db dyn Db,
        current: Option<Self>,
        previous: Option<Self>,
    ) -> Option<Self> {
        match (current, previous) {
            (None, None) => None,
            (Some(evidence), None) | (None, Some(evidence)) => Some(evidence),
            (Some(current), Some(previous)) if current == previous => Some(current),
            (Some(current), Some(previous)) => {
                let mut projection_evidence = ProjectionEvidenceBuilder::default();
                for fact in current
                    .projection_facts(db)
                    .iter()
                    .chain(previous.projection_facts(db))
                    .cloned()
                {
                    projection_evidence.push_projection_fact(fact);
                }

                for fact in current
                    .container_facts(db)
                    .iter()
                    .chain(previous.container_facts(db))
                    .cloned()
                {
                    projection_evidence.push_container_fact(fact);
                }

                projection_evidence.finish(db)
            }
        }
    }

    fn new(
        db: &'db dyn Db,
        projection_facts: FxIndexSet<ProjectionEvidenceFact<'db>>,
        container_facts: FxIndexSet<ProjectionContainerFact<'db>>,
    ) -> Option<Self> {
        (!projection_facts.is_empty() || !container_facts.is_empty()).then(|| {
            Self(ProjectionEvidenceSetInterned::new(
                db,
                projection_facts
                    .into_iter()
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
                container_facts
                    .into_iter()
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            ))
        })
    }

    fn projection_facts(self, db: &'db dyn Db) -> &'db [ProjectionEvidenceFact<'db>] {
        self.0.projection_facts(db)
    }

    fn container_facts(self, db: &'db dyn Db) -> &'db [ProjectionContainerFact<'db>] {
        self.0.container_facts(db)
    }

    /// Inference-time API: finds generic containers that may need recovery-time replay.
    fn generic_containers(
        db: &'db dyn Db,
        ty: Type<'db>,
    ) -> FxIndexSet<ProjectionContainerFact<'db>> {
        let facts = RefCell::new(FxIndexSet::default());
        any_over_type(db, ty, false, |nested| {
            if let Some(fact) = ProjectionContainerFact::try_from_inference_type(db, nested) {
                facts.borrow_mut().insert(fact);
            }
            false
        });
        facts.into_inner()
    }

    /// Cycle-recovery-time API: looks up a previously collected container fact.
    fn container_fact_for_arm(
        self,
        db: &'db dyn Db,
        root: DivergentType,
        arm: Type<'db>,
    ) -> Option<&'db ProjectionContainerFact<'db>> {
        let normalized_arm = arm
            .replace_projection_artifacts_with_root(db, root)
            .unwrap_or(arm);
        self.container_facts(db).iter().find(|fact| {
            if fact.arm == arm {
                return true;
            }

            let fact_arm = fact
                .arm
                .replace_projection_artifacts_with_root(db, root)
                .unwrap_or(fact.arm);
            fact_arm == normalized_arm
        })
    }

    /// Cycle-recovery-time API: replays a generic projection from inference-time evidence.
    fn project_generic_path(
        self,
        db: &'db dyn Db,
        root: DivergentType,
        arm: Type<'db>,
        path: &ProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        let normalized_arm = arm
            .replace_projection_artifacts_with_root(db, root)
            .unwrap_or(arm);
        self.projection_facts(db).iter().find_map(|fact| {
            if !fact.root.same_marker(root) || fact.path != *path {
                return None;
            }
            if fact.arm == arm {
                return Some(fact.term);
            }

            let fact_arm = fact
                .arm
                .replace_projection_artifacts_with_root(db, root)
                .unwrap_or(fact.arm);
            (fact_arm == normalized_arm).then_some(fact.term)
        })
    }
}

/// Interned storage for [`ProjectionEvidenceSet`].
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
struct ProjectionEvidenceSetInterned<'db> {
    #[returns(deref)]
    projection_facts: Box<[ProjectionEvidenceFact<'db>]>,
    #[returns(deref)]
    container_facts: Box<[ProjectionContainerFact<'db>]>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ProjectionEvidenceSetInterned<'_> {}

/// The result of projecting one generic container arm during inference.
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
struct ProjectionEvidenceFact<'db> {
    root: DivergentType,
    arm: Type<'db>,
    path: ProjectionPath<'db>,
    term: ProjectionTerm<'db>,
}

/// A generic container specialization computed during inference.
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
struct ProjectionContainerFact<'db> {
    arm: Type<'db>,
    class: StaticClassLiteral<'db>,
    arguments: Box<[Type<'db>]>,
}

impl<'db> ProjectionContainerFact<'db> {
    fn try_from_parts(
        arm: Type<'db>,
        class: StaticClassLiteral<'db>,
        arguments: &[Type<'db>],
    ) -> Option<Self> {
        (!arguments.is_empty()).then(|| Self {
            arm,
            class,
            arguments: arguments.to_vec().into_boxed_slice(),
        })
    }

    /// Cycle-recovery-time API: builds a fact from direct specialization only.
    fn try_from_recovery_type(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        if ty.exact_tuple_instance_spec(db).is_some() {
            return None;
        }

        let (class, specialization) = ty.direct_class_specialization(db)?;
        Self::try_from_parts(ty, class, specialization.types(db))
    }

    /// Inference-time API: builds a fact from the full specialization view.
    ///
    /// This may expand aliases, bounds, and fallbacks.
    fn try_from_inference_type(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        if ty.exact_tuple_instance_spec(db).is_some() {
            return None;
        }

        let (class, specialization) = ty.class_specialization(db)?;
        Self::try_from_parts(ty, class, specialization.types(db))
    }
}

/// A projected view of a cycle root produced while recovering recursive inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub struct ProjectionType<'db>(ProjectionTypeInterned<'db>);

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ProjectionType<'_> {}

impl<'db> ProjectionType<'db> {
    fn new(db: &'db dyn Db, root: DivergentType, path: ProjectionPath<'db>) -> Self {
        Self(ProjectionTypeInterned::new(db, root, path))
    }

    pub(crate) fn root(self, db: &'db dyn Db) -> DivergentType {
        self.0.root(db)
    }

    fn path(self, db: &'db dyn Db) -> ProjectionPath<'db> {
        self.0.path(db)
    }

    fn append(self, db: &'db dyn Db, op: ProjectionOp<'db>) -> Self {
        Self::new(db, self.root(db), self.path(db).append(op))
    }
}

/// Interned storage for [`ProjectionType`].
// Due to salsa restrictions, it is not possible to directly intern a public struct containing a private type.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
struct ProjectionTypeInterned<'db> {
    root: DivergentType,
    path: ProjectionPath<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ProjectionTypeInterned<'_> {}

/// An ordered sequence of projection operations applied to a cycle root.
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
struct ProjectionPath<'db> {
    ops: Box<[ProjectionOp<'db>]>,
}

impl<'db> ProjectionPath<'db> {
    fn from_op(op: ProjectionOp<'db>) -> Self {
        Self::from_ops([op])
    }

    fn from_ops(ops: impl IntoIterator<Item = ProjectionOp<'db>>) -> Self {
        Self {
            ops: ops.into_iter().collect::<Vec<_>>().into_boxed_slice(),
        }
    }

    fn ops(&self) -> &[ProjectionOp<'db>] {
        &self.ops
    }

    fn append(&self, op: ProjectionOp<'db>) -> Self {
        let mut ops = self.ops.to_vec();
        ops.push(op);
        Self {
            ops: ops.into_boxed_slice(),
        }
    }

    fn append_path(&self, path: &Self) -> Self {
        Self::from_ops(self.ops.iter().chain(path.ops.iter()).copied())
    }

    fn is_strict_prefix_of(&self, other: &Self) -> bool {
        self.ops.len() < other.ops.len() && other.ops.starts_with(&self.ops)
    }

    fn suffixes(&self) -> impl Iterator<Item = Self> + '_ {
        (0..self.ops.len()).map(|index| Self::from_ops(self.ops[index..].iter().copied()))
    }
}

/// An interned method name used by zero-argument method-call projections.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
struct ProjectionMethodName<'db> {
    #[returns(ref)]
    name: Name,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ProjectionMethodName<'_> {}

impl<'db> ProjectionMethodName<'db> {
    fn new(db: &'db dyn Db, name: &Name) -> Self {
        let mut name = name.clone();
        name.shrink_to_fit();
        Self::new_internal(db, name)
    }
}

/// An interned non-index subscript key type.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
struct ProjectionSubscriptKeyType<'db> {
    ty: Type<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ProjectionSubscriptKeyType<'_> {}

/// A single operation that can be preserved through cycle recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
enum ProjectionOp<'db> {
    Iter { is_async: bool },
    Unpack(UnpackProjection),
    Subscript(ProjectionSubscript<'db>),
    // There is no reason to target only 0-argument methods.
    // It would be nice to be able to scale it without compromising performance.
    CallMethod0(ProjectionMethodName<'db>),
    ContextEnter { is_async: bool },
    AwaitResult,
}

/// The fixed-length or starred-unpack projection of one unpacked position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
enum UnpackProjection {
    Exact {
        len: usize,
        index: usize,
    },
    Star {
        prefix: usize,
        suffix: usize,
        position: StarUnpackPosition,
    },
}

/// A subscript projection represented precisely enough for cycle recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
enum ProjectionSubscript<'db> {
    Unknown,
    Int,
    LiteralInt(i64),
    StaticSlice(StaticSliceProjection),
    KeyType(ProjectionSubscriptKeyType<'db>),
}

impl<'db> ProjectionSubscript<'db> {
    fn from_type(db: &'db dyn Db, slice_ty: Type<'db>) -> Option<Self> {
        if let Some(index) = slice_ty.as_int_like_literal() {
            return Some(Self::LiteralInt(index));
        }

        if let Some(slice) = slice_ty
            .as_nominal_instance()
            .and_then(|instance| instance.slice_literal(db))
            && slice.step != Some(0)
        {
            return Some(Self::StaticSlice(StaticSliceProjection::from(slice)));
        }

        if slice_ty.is_instance_of(db, KnownClass::Int)
            || slice_ty.is_instance_of(db, KnownClass::Bool)
        {
            return Some(Self::Int);
        }

        if slice_ty.is_dynamic() {
            return Some(Self::Unknown);
        }

        if slice_ty.is_instance_of(db, KnownClass::Slice) {
            return None;
        }

        Some(Self::KeyType(ProjectionSubscriptKeyType::new(db, slice_ty)))
    }

    fn to_type(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Self::Unknown => Type::unknown(),
            Self::Int => KnownClass::Int.to_instance(db),
            Self::LiteralInt(index) => Type::int_literal(index),
            Self::StaticSlice(slice) => KnownClass::Slice.to_specialized_instance(
                db,
                &[
                    slice.start.map_or_else(
                        || Type::none(db),
                        |value| Type::int_literal(i64::from(value)),
                    ),
                    slice.stop.map_or_else(
                        || Type::none(db),
                        |value| Type::int_literal(i64::from(value)),
                    ),
                    slice.step.map_or_else(
                        || Type::none(db),
                        |value| Type::int_literal(i64::from(value)),
                    ),
                ],
            ),
            Self::KeyType(key) => key.ty(db),
        }
    }
}

/// The projected position within a starred unpack pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
enum StarUnpackPosition {
    Prefix(usize),
    Rest,
    Suffix(usize),
}

/// A statically known `slice` value used by a subscript projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
struct StaticSliceProjection {
    start: Option<i32>,
    stop: Option<i32>,
    step: Option<i32>,
}

impl From<SliceLiteral> for StaticSliceProjection {
    fn from(slice: SliceLiteral) -> Self {
        Self {
            start: slice.start,
            stop: slice.stop,
            step: slice.step,
        }
    }
}
