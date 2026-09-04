use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ty_python_core::EvaluationMode;

use crate::Db;
use crate::types::call::CallArguments;
use crate::types::{
    DivergentType, MemberLookupPolicy, ProgramEnvironment, Type, UnionType,
    subscript::SubscriptError,
};

use super::artifact::{
    ProjectionCallArguments, ProjectionMember, ProjectionOp, ProjectionPath, ProjectionSubscript,
    StarUnpackPosition, UnpackProjection, new_projection_derivation,
};
use super::container::ProjectionContainer;
use super::equation::CycleRootSet;
use super::evidence::{ProjectionEvidenceBuilder, ProjectionEvidenceSet};
use super::term::ProjectionTerm;

impl<'db> Type<'db> {
    /// Inference-time API: projects an iterable value while recording cycle projection evidence.
    pub(crate) fn try_iter_projection_result_with_mode(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mode: EvaluationMode,
    ) -> Option<ProjectionResult<'db>> {
        let op = ProjectionOp::Iter {
            is_async: mode.is_async(),
        };
        self.try_projection_with_non_cycle_result(db, env, op, |ty| {
            ty.try_iterate_with_mode(db, env, mode)
                .ok()
                .map(|tuple| ProjectionTerm::Homogeneous(tuple.homogeneous_element_type(db, env)))
        })
    }

    /// Inference-time API: projects one target of an exact unpack operation.
    pub(crate) fn try_unpack_projection_result(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        len: usize,
        index: usize,
    ) -> Option<ProjectionResult<'db>> {
        let op = ProjectionOp::Unpack(UnpackProjection::Exact { len, index });
        self.try_projection_with_non_cycle_result(db, env, op, |ty| {
            ProjectionContainer::infer_projection_op(db, env, ty, op)
        })
    }

    /// Inference-time API: projects one fixed prefix target of a starred unpack operation.
    pub(crate) fn try_star_unpack_prefix_projection_result(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        prefix: usize,
        suffix: usize,
        index: usize,
    ) -> Option<ProjectionResult<'db>> {
        let op = ProjectionOp::Unpack(UnpackProjection::Star {
            prefix,
            suffix,
            position: StarUnpackPosition::Prefix(index),
        });
        self.try_projection_with_non_cycle_result(db, env, op, |ty| {
            ProjectionContainer::infer_projection_op(db, env, ty, op)
        })
    }

    /// Inference-time API: projects the list-valued rest target of a starred unpack operation.
    pub(crate) fn try_star_unpack_rest_projection_result(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        prefix: usize,
        suffix: usize,
    ) -> Option<ProjectionResult<'db>> {
        let op = ProjectionOp::Unpack(UnpackProjection::Star {
            prefix,
            suffix,
            position: StarUnpackPosition::Rest,
        });
        self.try_projection_with_non_cycle_result(db, env, op, |ty| {
            ProjectionContainer::infer_projection_op(db, env, ty, op)
        })
    }

    /// Inference-time API: projects one fixed suffix target of a starred unpack operation.
    pub(crate) fn try_star_unpack_suffix_projection_result(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        prefix: usize,
        suffix: usize,
        index: usize,
    ) -> Option<ProjectionResult<'db>> {
        let op = ProjectionOp::Unpack(UnpackProjection::Star {
            prefix,
            suffix,
            position: StarUnpackPosition::Suffix(index),
        });
        self.try_projection_with_non_cycle_result(db, env, op, |ty| {
            ProjectionContainer::infer_projection_op(db, env, ty, op)
        })
    }

    /// Inference-time API: projects a subscript operation without returning replay evidence.
    pub(crate) fn try_subscript_projection(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        slice_ty: Type<'db>,
    ) -> Option<Self> {
        self.try_subscript_projection_result(db, env, slice_ty)
            .map(ProjectionResult::ty)
    }

    /// Inference-time API: projects a subscript operation while recording cycle projection evidence.
    pub(crate) fn try_subscript_projection_result(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        slice_ty: Type<'db>,
    ) -> Option<ProjectionResult<'db>> {
        if !self.needs_projection_operation(db, env) {
            return None;
        }

        let subscript = ProjectionSubscript::from_type(db, slice_ty)?;
        let op = ProjectionOp::Subscript(subscript);
        self.try_projection_with_non_cycle_result(db, env, op, |ty| {
            ty.subscript(db, env, slice_ty, ast::ExprContext::Load)
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
        env: &ProgramEnvironment<'db>,
        slice_ty: Type<'db>,
        expr_context: ast::ExprContext,
    ) -> Option<Result<Self, SubscriptError<'db>>> {
        if !self.needs_projection_operation(db, env) {
            return None;
        }

        if !matches!(
            ProjectionSubscript::from_type(db, slice_ty)?,
            ProjectionSubscript::KeyType(_)
        ) {
            return None;
        }

        let result = self.subscript_without_projection(db, env, slice_ty, expr_context);
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
        env: &ProgramEnvironment<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> Option<ProjectionResult<'db>> {
        if !self.needs_projection_operation(db, env) {
            return None;
        }

        let op = ProjectionOp::Member(ProjectionMember::new(db, name, policy));
        self.try_projection_with_non_cycle_result(db, env, op, |ty| {
            ProjectionContainer::infer_member_type_for_type(db, env, ty, name, policy)
                .map(ProjectionTerm::Exact)
        })
    }

    /// Inference-time API: projects a call while recording cycle projection evidence.
    pub(crate) fn try_call_projection_result(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> Option<ProjectionResult<'db>> {
        if !self.needs_projection_operation(db, env) {
            return None;
        }

        let call_arguments = ProjectionCallArguments::new(db, arguments);
        let op = ProjectionOp::Call(call_arguments);
        self.try_projection_with_non_cycle_result(db, env, op, |ty| {
            ProjectionContainer::infer_call_type_for_type(db, env, ty, call_arguments)
                .map(ProjectionTerm::Exact)
        })
    }

    /// Inference-time API: projects a context-manager enter operation without replay evidence.
    pub(crate) fn try_context_enter_projection(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mode: EvaluationMode,
    ) -> Option<Self> {
        self.try_context_enter_projection_result(db, env, mode)
            .map(ProjectionResult::ty)
    }

    /// Inference-time API: projects a context-manager enter operation.
    pub(crate) fn try_context_enter_projection_result(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mode: EvaluationMode,
    ) -> Option<ProjectionResult<'db>> {
        let op = ProjectionOp::ContextEnter {
            is_async: mode.is_async(),
        };
        self.try_projection_with_non_cycle_result(db, env, op, |ty| {
            ty.try_enter_with_mode(db, env, mode)
                .ok()
                .map(ProjectionTerm::Exact)
        })
    }

    /// Inference-time API: projects the result of awaiting a value.
    pub(crate) fn try_await_projection_result(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<ProjectionResult<'db>> {
        self.try_projection_with_non_cycle_result(db, env, ProjectionOp::AwaitResult, |ty| {
            ty.try_await(db, env).ok().map(ProjectionTerm::Exact)
        })
    }

    fn try_projection_result(
        self,
        db: &'db dyn Db,
        op: ProjectionOp<'db>,
    ) -> Option<ProjectionResult<'db>> {
        match self {
            Type::Divergent(root) => {
                let (root, path) = super::projection_derivation(db, root).map_or_else(
                    || (root, ProjectionPath::from_op(op)),
                    |(root, path)| (root, path.append_path(&ProjectionPath::from_op(op))),
                );
                Some(ProjectionResult::new(Self::Divergent(
                    root.with_projection_derivation(new_projection_derivation(db, root, path)),
                )))
            }
            // Projection only exists while cycle recovery is active.
            Type::Projection(_) => None,
            _ => None,
        }
    }

    /// Inference-time helper for applying an operation to a type that may contain cycle markers.
    fn try_projection_with_non_cycle_result(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        op: ProjectionOp<'db>,
        mut project_non_cycle: impl FnMut(Self) -> Option<ProjectionTerm<'db>>,
    ) -> Option<ProjectionResult<'db>> {
        if !self.has_top_level_cycle_artifact(db) {
            return self.try_nested_cycle_projection_result(db, env, op, project_non_cycle);
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
                env,
                roots.iter().copied(),
                element,
                &path,
                term,
            );
            projected_non_cycle_elements.push((element, term.ty(db, env)));
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
            ty: UnionType::from_elements_cycle_recovery(db, env, elements),
            projection_evidence: projection_evidence.finish(db),
        })
    }

    /// Inference-time helper for projection artifacts nested below a top-level non-cycle shape.
    fn try_nested_cycle_projection_result(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        op: ProjectionOp<'db>,
        mut project_non_cycle: impl FnMut(Self) -> Option<ProjectionTerm<'db>>,
    ) -> Option<ProjectionResult<'db>> {
        let mut roots = self.projection_artifact_roots(db, env);
        // Bare divergent roots below a bridge container also need projection evidence. Unpack is
        // excluded because unpacking can be the operation that grows a recursive structure.
        if roots.is_empty() && !matches!(op, ProjectionOp::Unpack(_)) {
            roots = self.cycle_artifact_roots(db, env);
        }
        let [root] = roots.as_slice() else {
            return self.try_multi_root_nested_cycle_projection_result(
                db,
                env,
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
            if element.mentions_cycle_artifact_direct(db, env, *root) {
                recursive_elements.push((index, element));
                continue;
            }

            let term = project_non_cycle(element)?;
            projection_evidence.record_projected_arm(db, env, [*root], element, &path, term);
            terms[index] = Some(term);
        }

        let evidence = projection_evidence.finish(db);
        for (index, element) in recursive_elements {
            let container =
                ProjectionContainer::try_from(db, env, *root, element, evidence.as_ref())?;
            let term = container
                .project_inference_path(db, env, *root, evidence.as_ref(), &path)
                .or_else(|| {
                    if matches!(op, ProjectionOp::Subscript(_)) {
                        // The subscript path suppresses projection creation, so it can expose a
                        // flat dependency without recursively extending the projection cycle.
                        ProjectionContainer::infer_projection_op(db, env, element, op)
                    } else {
                        None
                    }
                })?;
            terms[index] = Some(term);
        }

        let terms = terms.into_iter().collect::<Option<Vec<_>>>()?;

        // These are operation results, not equations defining the cycle root. Preserve their
        // known structure even when some elements still depend on recursive inference.
        let ty = UnionType::from_elements_cycle_recovery(
            db,
            env,
            terms.into_iter().map(|term| term.ty(db, env)),
        );
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
        env: &ProgramEnvironment<'db>,
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
            let term = if element.mentions_cycle_artifact_in_roots(db, env, &root_set) {
                // Recursive arms must replay structurally. Re-running the full operation can
                // re-enter projection construction with the same recursive element.
                roots.iter().find_map(|root| {
                    let container = ProjectionContainer::try_from(db, env, *root, element, None)?;
                    container.project_inference_path(db, env, *root, None, &path)
                })?
            } else {
                project(element)?
            };
            projection_evidence.record_projected_arm(
                db,
                env,
                roots.iter().copied(),
                element,
                &path,
                term,
            );
            terms.push(term.ty(db, env));
        }

        Some(ProjectionResult {
            ty: UnionType::from_elements_cycle_recovery(db, env, terms),
            projection_evidence: projection_evidence.finish(db),
        })
    }
}

/// Inference-time result of a projection, plus facts needed to replay it during recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, salsa::SalsaValue, get_size2::GetSize)]
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
