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
    DivergentType, DynamicType, KnownClass, MemberLookupPolicy, StaticClassLiteral, TupleSpec,
    Type, UnionBuilder, UnionType, call::CallArguments, instance::SliceLiteral,
};
use crate::place::{DefinedPlace, Definedness, Place};
use crate::subscript::{PyIndex, PySlice};
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::tuple::{Tuple, TupleLength, TupleType};
use crate::types::visitor::any_over_type;
use crate::{Db, FxIndexMap, FxIndexSet};

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
        let subscript = ProjectionSubscript::from_type(db, slice_ty).or_else(|| {
            (!slice_ty.is_instance_of(db, KnownClass::Slice))
                .then_some(ProjectionSubscript::Unknown)
        })?;
        let op = ProjectionOp::Subscript(subscript);
        self.try_projection_with_non_cycle_result(db, op, |ty| {
            ty.subscript(db, slice_ty, ast::ExprContext::Load)
                .ok()
                .map(ProjectionTerm::Exact)
        })
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
        let mut roots = Vec::new();
        Self::collect_projection_artifact_roots(db, self, &mut roots);
        let [root] = roots.as_slice() else {
            return None;
        };

        let elements = self.top_level_projection_union_elements(db);
        let mut projection_evidence = ProjectionEvidenceBuilder::default();
        let path = ProjectionPath::from_op(op);
        let terms = elements
            .into_iter()
            .map(|element| {
                let mentions_root = element.mentions_cycle_artifact_direct(db, *root);
                let direct_container = ProjectionContainer::from_direct_type(db, element);

                let term = match (mentions_root, direct_container.as_ref()) {
                    (false, None) => return None,
                    (true, Some(container @ ProjectionContainer::Tuple { .. })) => {
                        // Project recursive tuple arms structurally. Calling the normal projection
                        // path here would reenter cycle recovery through tuple subscript handling.
                        container.project_path(db, *root, None, &path)?
                    }
                    (true, Some(ProjectionContainer::Generic(_))) => {
                        // Avoid reentering method or iteration inference for recursive generic
                        // arms. Subscript has a projection-suppressed path that can still expose
                        // the recursive element type without extending the projection cycle.
                        match op {
                            ProjectionOp::Subscript(_) => {
                                ProjectionContainer::infer_projection_op(db, element, op)?
                            }
                            _ => return None,
                        }
                    }
                    _ => project_non_cycle(element)?,
                };

                if !mentions_root
                    && matches!(direct_container, Some(ProjectionContainer::Generic(_)))
                    && !term.is_ambiguous(db)
                {
                    projection_evidence.push_projection_fact(ProjectionEvidenceFact {
                        root: *root,
                        arm: element,
                        path: path.clone(),
                        term,
                    });
                }
                Some(term)
            })
            .collect::<Option<Vec<_>>>()?;

        let ty = Self::solve_projection_terms(db, *root, &path, &terms)?;
        Some(ProjectionResult {
            ty,
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

    /// Cycle-recovery-time API: tries to solve projections rooted at the current cycle.
    pub(crate) fn try_projection_cycle_normalized(
        self,
        db: &'db dyn Db,
        root: DivergentType,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
    ) -> Option<Self> {
        let mut paths = Vec::new();
        Self::collect_projection_ops(db, root, self, &mut paths);
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

            let container = ProjectionContainer::from_recovery_type(db, root, *element, evidence)?;
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

        let solved_ops = ProjectionSystem::from_terms_by_op(db, root, &terms_by_op)?.solve(db)?;

        let elements = elements
            .into_iter()
            .filter(|element| {
                !matches!(element, Type::Divergent(divergent) if divergent.same_marker(root))
            })
            .map(|element| {
                Some((
                    element.replace_solved_projection_artifacts(db, root, &solved_ops)?,
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
        ProjectionSystem::from_terms_by_op(db, root, &terms_by_op)?
            .solve(db)?
            .shift_remove(path)
    }

    fn collect_projection_ops(
        db: &'db dyn Db,
        root: DivergentType,
        ty: Type<'db>,
        paths: &mut Vec<ProjectionPath<'db>>,
    ) {
        let mut demands = Vec::new();
        Self::collect_projection_demands(db, ty, &mut demands);
        for (candidate_root, path) in demands {
            if candidate_root.same_marker(root) && !paths.contains(&path) {
                paths.push(path);
            }
        }
    }

    fn collect_projection_demands(
        db: &'db dyn Db,
        ty: Type<'db>,
        demands: &mut Vec<(DivergentType, ProjectionPath<'db>)>,
    ) {
        let demands = RefCell::new(demands);
        any_over_type(db, ty, false, |nested| {
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
    }

    fn collect_cycle_artifact_roots(
        db: &'db dyn Db,
        ty: Type<'db>,
        roots: &mut Vec<DivergentType>,
    ) {
        Self::collect_cycle_artifact_roots_impl(db, ty, roots, true);
    }

    fn collect_projection_artifact_roots(
        db: &'db dyn Db,
        ty: Type<'db>,
        roots: &mut Vec<DivergentType>,
    ) {
        // Bare `Divergent` inside containers appears in recursive aliases too. Nested projection
        // recovery only starts from an already-recorded projection demand.
        Self::collect_cycle_artifact_roots_impl(db, ty, roots, false);
    }

    fn collect_cycle_artifact_roots_impl(
        db: &'db dyn Db,
        ty: Type<'db>,
        roots: &mut Vec<DivergentType>,
        include_divergent: bool,
    ) {
        match ty {
            Type::Divergent(root) if include_divergent => {
                Self::push_cycle_artifact_root(roots, root);
                return;
            }
            Type::Projection(projection) => {
                Self::push_cycle_artifact_root(roots, projection.root(db));
                return;
            }
            _ => {}
        }

        if let Type::Union(union) = ty {
            for element in union.elements(db) {
                Self::collect_cycle_artifact_roots_impl(db, *element, roots, include_divergent);
            }
            return;
        }

        if let Some(spec) = ty.exact_tuple_instance_spec(db) {
            for element in spec.as_ref().iter_all_elements() {
                Self::collect_cycle_artifact_roots_impl(db, element, roots, include_divergent);
            }
            return;
        }

        if let Some((_, specialization)) = ty.direct_class_specialization(db) {
            for argument in specialization.types(db) {
                Self::collect_cycle_artifact_roots_impl(db, *argument, roots, include_divergent);
            }
        }
    }

    fn push_cycle_artifact_root(roots: &mut Vec<DivergentType>, root: DivergentType) {
        if !roots.iter().any(|candidate| candidate.same_marker(root)) {
            roots.push(root);
        }
    }

    fn mentions_cycle_artifact_direct(self, db: &'db dyn Db, root: DivergentType) -> bool {
        let mut roots = Vec::new();
        Self::collect_cycle_artifact_roots(db, self, &mut roots);
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
                .map(|element| element.replace_solved_projection_artifacts(db, root, solved_ops))
                .collect::<Option<Vec<_>>>()?;
            return Some(UnionType::from_elements_cycle_recovery(db, elements));
        }

        if let Some(container) = ProjectionContainer::from_direct_type(db, self) {
            return container.into_type(db, root, solved_ops);
        }

        let mut paths = Vec::new();
        Self::collect_projection_ops(db, root, self, &mut paths);
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
        let mut paths = Vec::new();
        Self::collect_projection_ops(db, root, self, &mut paths);
        if paths.is_empty() {
            return Some(self);
        }

        let solved_ops = paths
            .into_iter()
            .map(|path| (path, Type::Divergent(root)))
            .collect::<FxIndexMap<_, _>>();
        self.replace_solved_projection_artifacts(db, root, &solved_ops)
    }

    fn mentions_cycle_artifact(self, db: &'db dyn Db, root: DivergentType) -> bool {
        any_over_type(db, self, false, |ty| match ty {
            Type::Divergent(divergent) => divergent.same_marker(root),
            Type::Projection(projection) => projection.root(db).same_marker(root),
            _ => false,
        })
    }

    fn mentions_foreign_cycle_artifact(self, db: &'db dyn Db, root: DivergentType) -> bool {
        any_over_type(db, self, false, |ty| match ty {
            Type::Divergent(divergent) => !divergent.same_marker(root),
            Type::Projection(projection) => !projection.root(db).same_marker(root),
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

    fn matching_projection_narrowing_var(
        self,
        db: &'db dyn Db,
        root: DivergentType,
    ) -> Option<ProjectionPath<'db>> {
        let Type::Intersection(intersection) = self else {
            return None;
        };

        let mut dependency = None;
        for positive in intersection.positive(db) {
            if let Type::Projection(projection) = positive
                && projection.root(db).same_marker(root)
            {
                if dependency
                    .as_ref()
                    .is_some_and(|path| path != &projection.path(db))
                {
                    return None;
                }
                dependency = Some(projection.path(db));
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

impl<'db> ProjectionContainer<'db> {
    /// Cycle-recovery-time API: builds a container shape from direct structure only.
    fn from_direct_type(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        if let Some(spec) = ty.exact_tuple_instance_spec(db) {
            return Some(Self::Tuple {
                spec: spec.as_ref().clone(),
            });
        }

        if let Some(fact) = ProjectionContainerFact::from_direct_type(db, ty) {
            return Some(Self::Generic(fact));
        }

        None
    }

    /// Cycle-recovery-time API: builds a container from direct structure or stored evidence.
    fn from_recovery_type(
        db: &'db dyn Db,
        root: DivergentType,
        ty: Type<'db>,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
    ) -> Option<Self> {
        Self::from_direct_type(db, ty).or_else(|| {
            let fact = evidence?.container_fact_for_arm(db, root, ty)?;
            Some(Self::Generic(fact.clone()))
        })
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
        let ty = match self {
            Self::Tuple { spec } => Type::tuple(TupleType::new(db, spec)),
            Self::Generic(fact) => {
                return evidence?.project_generic_path(db, root, fact.arm, path);
            }
        };
        Self::project_type_path(db, ty, root, evidence, path)
    }

    /// Cycle-recovery-time API: structurally replays a projection path against a type.
    fn project_type_path(
        db: &'db dyn Db,
        ty: Type<'db>,
        root: DivergentType,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
        path: &ProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        if let Type::Union(union) = ty {
            let terms = union
                .elements(db)
                .iter()
                .map(|element| Self::project_type_path(db, *element, root, evidence, path))
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

        Self::project_term_path(
            db,
            projected,
            root,
            evidence,
            &ProjectionPath::from_ops(tail.iter().copied()),
        )
    }

    fn project_term_path(
        db: &'db dyn Db,
        term: ProjectionTerm<'db>,
        root: DivergentType,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
        path: &ProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        let ProjectionTerm::List(element) = term else {
            return Self::project_type_path(db, term.ty(db), root, evidence, path);
        };

        // Preserve the list wrapper from starred unpacking while applying the tail path.
        // Converting to `list[T]` would require generic-container evidence for this synthetic list.
        let ops = path.ops();
        let (&op, tail) = ops.split_first()?;
        let projected = Self::project_list_op(db, element, op)?;
        if tail.is_empty() {
            return Some(projected);
        }

        Self::project_term_path(
            db,
            projected,
            root,
            evidence,
            &ProjectionPath::from_ops(tail.iter().copied()),
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
        subscript: ProjectionSubscript,
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
    ) -> Option<Type<'db>> {
        match self {
            Self::Tuple { spec } => match spec {
                Tuple::Fixed(tuple) => {
                    let elements = tuple
                        .iter_all_elements()
                        .map(|element| {
                            element.replace_solved_projection_artifacts(db, root, solved_ops)
                        })
                        .collect::<Option<Vec<_>>>()?;

                    Some(Type::heterogeneous_tuple(db, elements))
                }
                Tuple::Variable(tuple) => {
                    let prefix = tuple
                        .iter_prefix_elements()
                        .map(|element| {
                            element.replace_solved_projection_artifacts(db, root, solved_ops)
                        })
                        .collect::<Option<Vec<_>>>()?;
                    let variable = tuple
                        .variable()
                        .replace_solved_projection_artifacts(db, root, solved_ops)?;
                    let suffix = tuple
                        .iter_suffix_elements()
                        .map(|element| {
                            element.replace_solved_projection_artifacts(db, root, solved_ops)
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
                        (*argument).replace_solved_projection_artifacts(db, root, solved_ops)
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
struct ProjectionSystem<'db> {
    root: DivergentType,
    equations: FxIndexMap<ProjectionPath<'db>, ProjectionEquation<'db>>,
}

impl<'db> ProjectionSystem<'db> {
    fn from_terms_by_op(
        db: &'db dyn Db,
        root: DivergentType,
        terms_by_op: &FxIndexMap<ProjectionPath<'db>, Vec<ProjectionTerm<'db>>>,
    ) -> Option<Self> {
        let mut equations = FxIndexMap::default();
        for (path, terms) in terms_by_op {
            let mut equation = ProjectionEquation::default();
            for term in terms {
                equation.add_projection_term(db, root, path, *term)?;
            }
            equations.insert(path.clone(), equation);
        }

        Some(Self { root, equations })
    }

    fn solve(self, db: &'db dyn Db) -> Option<FxIndexMap<ProjectionPath<'db>, Type<'db>>> {
        let Self { root, equations } = self;
        if equations.is_empty() || equations.values().any(|equation| equation.unsupported) {
            return None;
        }

        let paths = equations.keys().cloned().collect::<Vec<_>>();
        let path_indices = paths
            .iter()
            .enumerate()
            .map(|(index, path)| (path.clone(), index))
            .collect::<FxIndexMap<_, _>>();

        let mut graph = vec![Vec::new(); paths.len()];
        for (path, equation) in &equations {
            let source = path_indices[path];
            for dependency in &equation.dependencies {
                graph[source].push(*path_indices.get(dependency)?);
            }
        }

        let sccs = dependency_first_strongly_connected_components(&graph);
        let mut solutions = vec![None; paths.len()];
        for scc in sccs {
            let wrap_in_list = equations[&paths[*scc.first()?]].wrap_in_list?;
            for &index in &scc {
                if equations[&paths[index]].wrap_in_list != Some(wrap_in_list) {
                    return None;
                }
            }

            if scc.iter().any(|&index| equations[&paths[index]].divergent) {
                // This deliberately loses productive terms in the SCC. Keeping them would require
                // representing recursive solutions such as `A = int | list[A]`; `Divergent` is the
                // conservative widening value that avoids under-approximating the shape as `int`.
                let solved = if wrap_in_list {
                    KnownClass::List.to_specialized_instance(db, &[Type::Divergent(root)])
                } else {
                    Type::Divergent(root)
                };
                for index in scc {
                    solutions[index] = Some(solved);
                }
                continue;
            }

            let mut base = Vec::new();
            for &index in &scc {
                let equation = &equations[&paths[index]];
                base.extend(equation.productive.iter().copied());
                for dependency in &equation.dependencies {
                    let dependency_index = path_indices[dependency];
                    if !scc.contains(&dependency_index) {
                        base.push(solutions[dependency_index]?);
                    }
                }
            }

            if base.is_empty() {
                return None;
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

        paths
            .into_iter()
            .enumerate()
            .map(|(index, path)| Some((path, solutions[index]?)))
            .collect()
    }
}

/// One projection equation `A_p = productive | dependencies`.
#[derive(Default)]
struct ProjectionEquation<'db> {
    productive: Vec<Type<'db>>,
    dependencies: FxIndexSet<ProjectionPath<'db>>,
    divergent: bool,
    unsupported: bool,
    wrap_in_list: Option<bool>,
}

impl<'db> ProjectionEquation<'db> {
    fn add_projection_term(
        &mut self,
        db: &'db dyn Db,
        root: DivergentType,
        path: &ProjectionPath<'db>,
        term: ProjectionTerm<'db>,
    ) -> Option<()> {
        let wrap_in_list = matches!(term, ProjectionTerm::List(_));
        match self.wrap_in_list {
            Some(existing) if existing != wrap_in_list => return None,
            None => self.wrap_in_list = Some(wrap_in_list),
            Some(_) => {}
        }

        match term {
            ProjectionTerm::Exact(term) => {
                // The term may still be an unprojected recursive candidate like
                // `tuple[Projection_{path}(D), T]`; project it before collecting
                // productive parts for `Projection_{path}(D)`.
                if term.mentions_matching_projection(db, root, path)
                    && let Some(projected) =
                        ProjectionContainer::project_type_path(db, term, root, None, path)
                {
                    if projected.ty(db).is_matching_projection(db, root, path) {
                        self.divergent = true;
                        return Some(());
                    }
                    return self.add_projection_term(db, root, path, projected);
                }

                self.add_type_term(db, root, term, true)
            }
            ProjectionTerm::Homogeneous(term) => self.add_type_term(db, root, term, true),
            ProjectionTerm::List(term) => self.add_type_term(db, root, term, false),
        }
    }

    fn add_type_term(
        &mut self,
        db: &'db dyn Db,
        root: DivergentType,
        term: Type<'db>,
        allow_dependencies: bool,
    ) -> Option<()> {
        if let Type::Union(union) = term {
            for element in union.elements(db) {
                self.add_type_term(db, root, *element, allow_dependencies)?;
            }
            return Some(());
        }

        if allow_dependencies {
            if let Type::Projection(projection) = term
                && projection.root(db).same_marker(root)
            {
                self.dependencies.insert(projection.path(db));
                return Some(());
            }

            if let Some(path) = term.matching_projection_narrowing_var(db, root) {
                // Recovery treats `A & C` as the dependency `A`. This is a safe widening; retaining
                // `C` would require recovery-only intersection simplification.
                self.dependencies.insert(path);
                return Some(());
            }
        }

        if term.mentions_foreign_cycle_artifact(db, root) {
            self.unsupported = true;
            return Some(());
        }

        if term.mentions_cycle_artifact(db, root) {
            // A same-root occurrence below a constructor is a true recursive shape, not a flat
            // self-reference. Use the cycle root as the widening point instead of handing the
            // growing constructor chain back to fixed-point iteration.
            self.divergent = true;
            return Some(());
        }

        self.productive.push(term);
        Some(())
    }
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
            let mut demands = Vec::new();
            Type::collect_projection_demands(db, ty, &mut demands);
            if demands.is_empty() {
                continue;
            }

            let mut generic_containers = FxIndexSet::default();
            ProjectionEvidenceSet::collect_generic_containers(db, ty, &mut generic_containers);
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

        let Some(container_fact) = ProjectionContainerFact::from_inference_type(db, arm) else {
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
    /// Inference-time API: collects projection evidence for later cycle recovery.
    pub(crate) fn from_types(
        db: &'db dyn Db,
        types: impl IntoIterator<Item = Type<'db>>,
    ) -> Option<Self> {
        let mut builder = ProjectionEvidenceBuilder::default();
        builder.extend_from_types(db, types);
        builder.finish(db)
    }

    /// Inference-time API: conditionally collects projection evidence.
    pub(crate) fn from_types_if_needed(
        db: &'db dyn Db,
        should_collect: bool,
        types: impl IntoIterator<Item = Type<'db>>,
    ) -> Option<Self> {
        should_collect
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
    fn collect_generic_containers(
        db: &'db dyn Db,
        ty: Type<'db>,
        facts: &mut FxIndexSet<ProjectionContainerFact<'db>>,
    ) {
        let facts = RefCell::new(facts);
        any_over_type(db, ty, false, |nested| {
            if let Some(fact) = ProjectionContainerFact::from_inference_type(db, nested) {
                facts.borrow_mut().insert(fact);
            }
            false
        });
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
    fn from_parts(
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

    /// Cycle-recovery-time API: builds a fact using only direct class structure.
    fn from_direct_type(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        if ty.exact_tuple_instance_spec(db).is_some() {
            return None;
        }

        let (class, specialization) = ty.direct_class_specialization(db)?;
        Self::from_parts(ty, class, specialization.types(db))
    }

    /// Inference-time API: builds a fact from the full specialization view.
    ///
    /// This may expand aliases, bounds, and fallbacks.
    fn from_inference_type(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        if ty.exact_tuple_instance_spec(db).is_some() {
            return None;
        }

        let (class, specialization) = ty.class_specialization(db)?;
        Self::from_parts(ty, class, specialization.types(db))
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

/// A single operation that can be preserved through cycle recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
enum ProjectionOp<'db> {
    Iter { is_async: bool },
    Unpack(UnpackProjection),
    Subscript(ProjectionSubscript),
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
enum ProjectionSubscript {
    Unknown,
    Int,
    LiteralInt(i64),
    StaticSlice(StaticSliceProjection),
}

impl ProjectionSubscript {
    fn from_type(db: &dyn Db, slice_ty: Type<'_>) -> Option<Self> {
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

        None
    }

    fn to_type(self, db: &dyn Db) -> Type<'_> {
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
