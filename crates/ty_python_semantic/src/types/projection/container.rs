use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ty_python_core::EvaluationMode;

use crate::place::{DefinedPlace, Definedness, Place};
use crate::subscript::{PyIndex, PySlice};
use crate::types::tuple::{Tuple, TupleLength, TupleType};
use crate::types::{
    DivergentType, KnownClass, MemberLookupPolicy, TupleSpec, Type, UnionType, call::CallArguments,
};
use crate::{Db, FxIndexMap};

use super::ProjectionType;
use super::artifact::{
    ProjectionOp, ProjectionPath, ProjectionSubscript, StarUnpackPosition, UnpackProjection,
};
use super::evidence::{ProjectionContainerFact, ProjectionEvidenceSet};
use super::term::ProjectionTerm;

/// A container shape that can explain projections of a cycle root.
#[derive(Debug, Clone)]
pub(super) enum ProjectionContainer<'db> {
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
    pub(super) fn try_from(
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
    pub(super) fn collect_projection_terms(
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
    pub(super) fn project_path(
        &self,
        db: &'db dyn Db,
        root: DivergentType,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
        path: &ProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        self.project_path_impl(db, root, evidence, path, ProjectionReplayMode::SingleRoot)
    }

    pub(super) fn project_multi_root_path(
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
                    .and_then(|evidence| evidence.project_arm_path(db, root, fact.arm, path))
                {
                    return Some(term);
                }

                return None;
            }
        };
        Self::project_type_path_impl(db, ty, root, evidence, path, mode)
    }

    /// Cycle-recovery-time API: structurally replays a projection path against a type.
    pub(super) fn project_type_path(
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

    pub(super) fn project_multi_root_type_path(
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
            evidence.and_then(|evidence| evidence.project_arm_path(db, root, ty, path))
        {
            return Some(term);
        }

        let ops = path.ops();
        let (&op, tail) = ops.split_first()?;

        let single_op_path = ProjectionPath::from_op(op);
        let projected = evidence
            .and_then(|evidence| evidence.project_arm_path(db, root, ty, &single_op_path))
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

    pub(super) fn project_list_op(
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
            ProjectionOp::Member(_)
            | ProjectionOp::CallMethod0(_)
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
            ProjectionOp::Member(_)
            | ProjectionOp::CallMethod0(_)
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

    pub(super) fn into_type(
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

    pub(super) fn infer_method_call0_type_for_type(
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

    pub(super) fn infer_member_type_for_type(
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

    pub(super) fn infer_projection_op(
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
            ProjectionOp::Member(member) => Some(ProjectionTerm::Exact(
                Self::infer_member_type_for_type(db, ty, member.name(db), member.policy())?,
            )),
            ProjectionOp::CallMethod0(method) => Some(ProjectionTerm::Exact(
                Self::infer_method_call0_type_for_type(db, ty, method.as_name(db))?,
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
