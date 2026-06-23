use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ty_python_core::EvaluationMode;

use crate::Db;
use crate::types::{DivergentType, MemberLookupPolicy, Type, UnionType, subscript::SubscriptError};

use super::artifact::{
    ProjectionMember, ProjectionMemberName, ProjectionOp, ProjectionPath, ProjectionSubscript,
    ProjectionType, StarUnpackPosition, UnpackProjection,
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
        if !self.needs_projection_operation(db) {
            return None;
        }

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
        if !self.needs_projection_operation(db) {
            return None;
        }

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
        if !self.needs_projection_operation(db) {
            return None;
        }

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
        if !self.needs_projection_operation(db) {
            return None;
        }

        let op = ProjectionOp::CallMethod0(ProjectionMemberName::new(db, method_name));
        self.try_projection_with_non_cycle_result(db, op, |ty| {
            ProjectionContainer::infer_method_call0_type_for_type(db, ty, method_name)
                .map(ProjectionTerm::Exact)
        })
    }

    /// Inference-time API: restores unresolved member projections in method-call callee position.
    ///
    /// A method call first infers the attribute access used as its callee. If that intermediate
    /// callee contains an unresolved `Member` projection, fall back to the pre-projection behavior
    /// where `Divergent.foo(...)` remains `Divergent`, instead of allowing the intermediate bound
    /// method object to affect the return type.
    pub(crate) fn member_projection_callee_fallback(
        self,
        db: &'db dyn Db,
        method_name: &Name,
    ) -> Self {
        if !self.has_top_level_cycle_artifact(db) {
            return self;
        }

        match self {
            Type::Projection(projection)
                if projection.path(db).ops().last().is_some_and(|op| {
                    matches!(
                        op,
                        ProjectionOp::Member(member) if member.name(db) == method_name
                    )
                }) =>
            {
                Type::Divergent(projection.root(db))
            }
            Type::Union(union) => union.map(db, |element| {
                element.member_projection_callee_fallback(db, method_name)
            }),
            _ => self,
        }
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
