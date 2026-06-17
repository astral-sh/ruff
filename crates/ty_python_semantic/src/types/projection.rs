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
    Type, UnionType, call::CallArguments, instance::SliceLiteral,
};
use crate::Db;
use crate::place::{DefinedPlace, Definedness, Place};
use crate::types::tuple::{Tuple, TupleLength, TupleType};
use crate::types::visitor::any_over_type;

impl<'db> Type<'db> {
    pub(crate) fn try_iter_projection_with_mode(
        self,
        db: &'db dyn Db,
        mode: EvaluationMode,
    ) -> Option<Self> {
        let op = ProjectionOp::Iter {
            is_async: mode.is_async(),
        };
        self.try_projection_with_non_cycle(db, op, |ty| {
            ty.try_iterate_with_mode(db, mode)
                .ok()
                .map(|tuple| tuple.homogeneous_element_type(db))
        })
    }

    pub(crate) fn try_unpack_projection(
        self,
        db: &'db dyn Db,
        len: usize,
        index: usize,
    ) -> Option<Self> {
        self.try_projection(
            db,
            ProjectionOp::Unpack(UnpackProjection::Exact { len, index }),
        )
    }

    pub(crate) fn try_star_unpack_prefix_projection(
        self,
        db: &'db dyn Db,
        prefix: usize,
        suffix: usize,
        index: usize,
    ) -> Option<Self> {
        self.try_projection(
            db,
            ProjectionOp::Unpack(UnpackProjection::Star {
                prefix,
                suffix,
                position: StarUnpackPosition::Prefix(index),
            }),
        )
    }

    pub(crate) fn try_star_unpack_rest_projection(
        self,
        db: &'db dyn Db,
        prefix: usize,
        suffix: usize,
    ) -> Option<Self> {
        self.try_projection(
            db,
            ProjectionOp::Unpack(UnpackProjection::Star {
                prefix,
                suffix,
                position: StarUnpackPosition::Rest,
            }),
        )
    }

    pub(crate) fn try_star_unpack_suffix_projection(
        self,
        db: &'db dyn Db,
        prefix: usize,
        suffix: usize,
        index: usize,
    ) -> Option<Self> {
        self.try_projection(
            db,
            ProjectionOp::Unpack(UnpackProjection::Star {
                prefix,
                suffix,
                position: StarUnpackPosition::Suffix(index),
            }),
        )
    }

    pub(crate) fn try_subscript_projection(
        self,
        db: &'db dyn Db,
        slice_ty: Type<'db>,
    ) -> Option<Self> {
        let subscript = ProjectionSubscript::from_type(db, slice_ty).or_else(|| {
            (!slice_ty.is_instance_of(db, KnownClass::Slice))
                .then_some(ProjectionSubscript::Unknown)
        })?;
        let op = ProjectionOp::Subscript(subscript);
        self.try_projection_with_non_cycle(db, op, |ty| {
            ty.subscript(db, slice_ty, ast::ExprContext::Load).ok()
        })
    }

    pub(crate) fn try_method_call_projection(
        self,
        db: &'db dyn Db,
        method_name: &Name,
    ) -> Option<Self> {
        let op = ProjectionOp::CallMethod0(ProjectionMethodName::new(db, method_name));
        self.try_projection_with_non_cycle(db, op, |ty| {
            ProjectionContainer::infer_method_call0_type_for_type(db, ty, method_name)
        })
    }

    pub(crate) fn try_context_enter_projection(
        self,
        db: &'db dyn Db,
        mode: EvaluationMode,
    ) -> Option<Self> {
        let op = ProjectionOp::ContextEnter {
            is_async: mode.is_async(),
        };
        self.try_projection_with_non_cycle(db, op, |ty| ty.try_enter_with_mode(db, mode).ok())
    }

    pub(crate) fn try_await_projection(self, db: &'db dyn Db) -> Option<Self> {
        self.try_projection_with_non_cycle(db, ProjectionOp::AwaitResult, |ty| {
            ty.try_await(db).ok()
        })
    }

    fn try_projection(self, db: &'db dyn Db, op: ProjectionOp<'db>) -> Option<Self> {
        match self {
            Type::Divergent(root) => Some(Self::Projection(ProjectionType::new(
                db,
                root,
                ProjectionPath::from_op(op),
            ))),
            Type::Projection(projection) => Some(Self::Projection(projection.append(db, op))),
            _ => None,
        }
    }

    fn try_projection_with_non_cycle(
        self,
        db: &'db dyn Db,
        op: ProjectionOp<'db>,
        mut project_non_cycle: impl FnMut(Self) -> Option<Self>,
    ) -> Option<Self> {
        if !self.has_top_level_cycle_artifact(db) {
            return None;
        }

        let Type::Union(union) = self else {
            return self.try_projection(db, op);
        };

        let mut saw_projection = false;
        let mut elements = Vec::new();

        for element in union.elements(db).iter().copied() {
            if let Some(projected) = element.try_projection(db, op) {
                saw_projection = true;
                elements.push(projected);
            } else {
                elements.push(project_non_cycle(element)?);
            }
        }

        saw_projection.then(|| UnionType::from_elements_cycle_recovery(db, elements))
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

    pub(crate) fn try_projection_cycle_normalized(
        self,
        db: &'db dyn Db,
        root: DivergentType,
    ) -> Option<Self> {
        if !self.contains_projection_for_root(db, root) {
            return None;
        }

        let evidence = CycleRecoveryEvidence::from_type(db, root, self);
        self.try_container_projection_cycle_normalized(db, root, &evidence)
    }

    /// Solves all projections of `root` that can be explained by top-level containers.
    ///
    /// The solver works in four steps:
    ///
    /// 1. Split the candidate recursive type into its top-level union arms and
    ///    collect every projection path rooted at `root`.
    /// 2. Treat non-root union arms as container evidence. Each supported arm
    ///    must be able to project every collected path.
    /// 3. Solve each path from the terms produced by those containers, dropping
    ///    recursive self references and unioning the remaining productive terms.
    /// 4. Rebuild the original top-level arms with every cycle artifact replaced
    ///    by its solved projection type.
    ///
    /// Returning `None` means that this recovery step cannot make progress
    /// without losing information; Salsa cycle recovery can then keep iterating
    /// toward a wider fixed point.
    fn try_container_projection_cycle_normalized(
        self,
        db: &'db dyn Db,
        root: DivergentType,
        evidence: &[CycleRecoveryEvidence<'db>],
    ) -> Option<Self> {
        let elements = self.top_level_projection_union_elements(db);
        let mut containers = Vec::new();
        let mut ops = Vec::new();

        for element in &elements {
            Self::collect_projection_ops(db, root, *element, &mut ops);

            if element.same_divergent_marker(db, Type::Divergent(root)) {
                continue;
            }

            let container = ProjectionContainer::from_type(db, *element)?;
            containers.push(container);
        }

        if containers.is_empty() || ops.is_empty() {
            return None;
        }

        let mut terms_by_op = ops
            .iter()
            .cloned()
            .map(|path| (path, Vec::new()))
            .collect::<Vec<_>>();
        for container in &containers {
            container.collect_projection_terms(db, evidence, &mut terms_by_op)?;
        }

        let solved_ops = terms_by_op
            .iter()
            .map(|(path, terms)| {
                Some((
                    path.clone(),
                    Self::solve_projection_terms(db, root, path, terms)?,
                ))
            })
            .collect::<Option<Vec<_>>>()?;

        let elements = elements
            .into_iter()
            .filter(|element| {
                !matches!(element, Type::Divergent(divergent) if divergent.same_marker(root))
            })
            .map(|element| element.replace_solved_projection_artifacts(db, root, &solved_ops))
            .collect::<Option<Vec<_>>>()?;

        Some(UnionType::from_elements_cycle_recovery(db, elements))
    }

    fn top_level_projection_union_elements(self, db: &'db dyn Db) -> Vec<Self> {
        match self {
            Type::Union(union) => union.elements(db).to_vec(),
            _ => vec![self],
        }
    }

    /// Solves the candidate terms for one projection path.
    ///
    /// Exact terms are checked for incompatible nested projections, while
    /// homogeneous and list terms only contribute element types. Recursive
    /// references to the same root are ignored when at least one non-recursive
    /// term remains. If a path has no productive term, it solves to `Never`
    /// only when it is not purely self-recursive. Star-unpack rest terms solve
    /// to `list[T]`, so they cannot be mixed with scalar projection terms.
    fn solve_projection_terms(
        db: &'db dyn Db,
        root: DivergentType,
        path: &ProjectionPath<'db>,
        terms: &[ProjectionTerm<'db>],
    ) -> Option<Self> {
        let mut saw_self_reference = false;
        let mut productive_terms = Vec::new();
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

        for term in terms {
            match *term {
                ProjectionTerm::Exact(term) => {
                    Self::collect_projection_component_terms(
                        db,
                        root,
                        path,
                        term,
                        &mut saw_self_reference,
                        &mut productive_terms,
                    )?;
                }
                ProjectionTerm::Homogeneous(term) => {
                    Self::collect_homogeneous_projection_component_terms(
                        db,
                        root,
                        term,
                        &mut saw_self_reference,
                        &mut productive_terms,
                    );
                }
                ProjectionTerm::List(term) => {
                    Self::collect_homogeneous_projection_component_terms(
                        db,
                        root,
                        term,
                        &mut saw_self_reference,
                        &mut productive_terms,
                    );
                }
            }
        }

        if productive_terms.is_empty() {
            return (!saw_self_reference).then(|| {
                if wrap_in_list {
                    KnownClass::List.to_specialized_instance(db, &[Type::Never])
                } else {
                    Type::Never
                }
            });
        }

        let solved = match productive_terms.as_slice() {
            [term] => *term,
            _ => UnionType::from_elements_cycle_recovery(db, productive_terms),
        };

        Some(if wrap_in_list {
            KnownClass::List.to_specialized_instance(db, &[solved])
        } else {
            solved
        })
    }

    fn collect_homogeneous_projection_component_terms(
        db: &'db dyn Db,
        root: DivergentType,
        term: Type<'db>,
        saw_self_reference: &mut bool,
        productive_terms: &mut Vec<Type<'db>>,
    ) {
        if let Type::Union(union) = term {
            for element in union.elements(db) {
                Self::collect_homogeneous_projection_component_terms(
                    db,
                    root,
                    *element,
                    saw_self_reference,
                    productive_terms,
                );
            }
            return;
        }

        if term.mentions_cycle_artifact(db, root) {
            *saw_self_reference = true;
            return;
        }

        Self::add_productive_projection_term(db, term, productive_terms);
    }

    fn collect_projection_component_terms(
        db: &'db dyn Db,
        root: DivergentType,
        path: &ProjectionPath<'db>,
        term: Type<'db>,
        saw_self_reference: &mut bool,
        productive_terms: &mut Vec<Type<'db>>,
    ) -> Option<()> {
        if let Type::Union(union) = term {
            for element in union.elements(db) {
                Self::collect_projection_component_terms(
                    db,
                    root,
                    path,
                    *element,
                    saw_self_reference,
                    productive_terms,
                )?;
            }
            return Some(());
        }

        if term.mentions_nonmatching_projection(db, root, path) {
            return None;
        }

        if term.mentions_cycle_artifact(db, root) {
            *saw_self_reference = true;
            return Some(());
        }

        Self::add_productive_projection_term(db, term, productive_terms);
        Some(())
    }

    fn collect_projection_ops(
        db: &'db dyn Db,
        root: DivergentType,
        ty: Type<'db>,
        paths: &mut Vec<ProjectionPath<'db>>,
    ) {
        let paths = RefCell::new(paths);
        any_over_type(db, ty, false, |nested| {
            if let Type::Projection(projection) = nested
                && projection.root(db).same_marker(root)
            {
                let mut paths = paths.borrow_mut();
                let path = projection.path(db);
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
            false
        });
    }

    fn solved_projection_type(
        solved_ops: &[(ProjectionPath<'db>, Type<'db>)],
        path: &ProjectionPath<'db>,
    ) -> Option<Self> {
        solved_ops
            .iter()
            .find_map(|(candidate, ty)| (candidate == path).then_some(*ty))
    }

    fn replace_solved_projection_artifacts(
        self,
        db: &'db dyn Db,
        root: DivergentType,
        solved_ops: &[(ProjectionPath<'db>, Type<'db>)],
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

        if let Some(container) = ProjectionContainer::from_type(db, self) {
            return container.into_type(db, root, solved_ops);
        }

        let mut paths = Vec::new();
        Self::collect_projection_ops(db, root, self, &mut paths);
        match paths.as_slice() {
            [path] => Self::solved_projection_type(solved_ops, path),
            _ => None,
        }
    }

    fn add_productive_projection_term(
        db: &'db dyn Db,
        term: Type<'db>,
        productive_terms: &mut Vec<Type<'db>>,
    ) {
        if productive_terms
            .iter()
            .any(|existing| Self::projection_term_absorbs(db, *existing, term))
        {
            return;
        }

        productive_terms.retain(|existing| !Self::projection_term_absorbs(db, term, *existing));
        productive_terms.push(term);
    }

    fn projection_term_absorbs(
        db: &'db dyn Db,
        possible_base: Type<'db>,
        possible_subtype: Type<'db>,
    ) -> bool {
        if possible_base == possible_subtype {
            return true;
        }

        let Type::Intersection(intersection) = possible_subtype else {
            return false;
        };

        if intersection.positive(db).is_empty() {
            possible_base == Type::object()
        } else {
            intersection.positive(db).contains(&possible_base)
        }
    }

    fn contains_projection_for_root(self, db: &'db dyn Db, root: DivergentType) -> bool {
        any_over_type(db, self, false, |ty| match ty {
            Type::Projection(projection) => projection.root(db).same_marker(root),
            _ => false,
        })
    }

    fn mentions_cycle_artifact(self, db: &'db dyn Db, root: DivergentType) -> bool {
        any_over_type(db, self, false, |ty| match ty {
            Type::Divergent(divergent) => divergent.same_marker(root),
            Type::Projection(projection) => projection.root(db).same_marker(root),
            _ => false,
        })
    }

    fn mentions_nonmatching_projection(
        self,
        db: &'db dyn Db,
        root: DivergentType,
        path: &ProjectionPath<'db>,
    ) -> bool {
        any_over_type(db, self, false, |ty| match ty {
            Type::Projection(projection) => {
                projection.root(db).same_marker(root) && projection.path(db).ne(path)
            }
            _ => false,
        })
    }
}

/// A container shape that can explain projections of a cycle root.
#[derive(Debug, Clone)]
enum ProjectionContainer<'db> {
    Tuple {
        spec: TupleSpec<'db>,
    },
    Generic {
        class: StaticClassLiteral<'db>,
        arguments: Vec<Type<'db>>,
        arm: Type<'db>,
    },
}

impl<'db> ProjectionContainer<'db> {
    fn from_type(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        if let Some(spec) = ty.exact_tuple_instance_spec(db) {
            return Some(Self::Tuple {
                spec: spec.as_ref().clone(),
            });
        }

        if let Some((class, specialization)) = ty.class_specialization(db) {
            let arguments = specialization.types(db);
            if !arguments.is_empty() {
                return Some(Self::Generic {
                    class,
                    arguments: arguments.to_vec(),
                    arm: ty,
                });
            }
        }

        None
    }

    fn collect_projection_terms(
        &self,
        db: &'db dyn Db,
        evidence: &[CycleRecoveryEvidence<'db>],
        terms_by_op: &mut [(ProjectionPath<'db>, Vec<ProjectionTerm<'db>>)],
    ) -> Option<()> {
        for (path, terms) in terms_by_op {
            terms.push(self.project_path(db, evidence, path)?);
        }
        Some(())
    }

    fn project_path(
        &self,
        db: &'db dyn Db,
        evidence: &[CycleRecoveryEvidence<'db>],
        path: &ProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        let ty = match self {
            Self::Tuple { spec } => Type::tuple(TupleType::new(db, spec)),
            Self::Generic { arm, .. } => *arm,
        };
        Self::project_type_path(db, ty, evidence, path)
    }

    fn project_type_path(
        db: &'db dyn Db,
        ty: Type<'db>,
        evidence: &[CycleRecoveryEvidence<'db>],
        path: &ProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        if let Some(term) = Self::project_custom_path(db, ty, evidence, path) {
            return Some(term);
        }

        let ops = path.ops();
        let (&op, tail) = ops.split_first()?;

        let projected = match op {
            ProjectionOp::Iter { is_async } => Self::infer_iter_item(db, ty, is_async)?,
            ProjectionOp::Unpack(unpack) => Self::infer_unpack(db, ty, unpack)?,
            ProjectionOp::Subscript(subscript) => Self::infer_subscript(db, ty, subscript)?,
            ProjectionOp::CallMethod0(method) => Self::infer_method_call0(db, ty, method)?,
            ProjectionOp::ContextEnter { is_async } => Self::infer_context_enter(db, ty, is_async)?,
            ProjectionOp::AwaitResult => Self::infer_await_result(db, ty)?,
        };

        if tail.is_empty() {
            return Some(projected);
        }

        Self::project_type_path(
            db,
            projected.ty(db),
            evidence,
            &ProjectionPath::from_ops(tail.iter().copied()),
        )
    }

    fn project_custom_path(
        db: &'db dyn Db,
        ty: Type<'db>,
        evidence: &[CycleRecoveryEvidence<'db>],
        path: &ProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        Self::is_custom_generic_container(db, ty).then_some(())?;
        evidence
            .iter()
            .find_map(|fact| (fact.arm == ty && fact.path.eq(path)).then_some(fact.term))
    }

    fn infer_projection_path(
        db: &'db dyn Db,
        ty: Type<'db>,
        ops: &[ProjectionOp<'db>],
    ) -> Option<ProjectionTerm<'db>> {
        let (&op, tail) = ops.split_first()?;

        let projected = match op {
            ProjectionOp::Iter { is_async } => Self::infer_iter_item(db, ty, is_async)?,
            ProjectionOp::Unpack(unpack) => Self::infer_unpack(db, ty, unpack)?,
            ProjectionOp::Subscript(subscript) => Self::infer_subscript(db, ty, subscript)?,
            ProjectionOp::CallMethod0(method) => Self::infer_method_call0(db, ty, method)?,
            ProjectionOp::ContextEnter { is_async } => Self::infer_context_enter(db, ty, is_async)?,
            ProjectionOp::AwaitResult => Self::infer_await_result(db, ty)?,
        };

        if tail.is_empty() {
            return Some(projected);
        }

        Self::infer_projection_path(db, projected.ty(db), tail)
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

    fn infer_method_call0(
        db: &'db dyn Db,
        ty: Type<'db>,
        method: ProjectionMethodName<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        Some(ProjectionTerm::Exact(
            Self::infer_method_call0_type_for_type(db, ty, method.name(db))?,
        ))
    }

    fn infer_iter_item(
        db: &'db dyn Db,
        ty: Type<'db>,
        is_async: bool,
    ) -> Option<ProjectionTerm<'db>> {
        let mode = if is_async {
            EvaluationMode::Async
        } else {
            EvaluationMode::Sync
        };

        Some(ProjectionTerm::Homogeneous(
            ty.try_iterate_with_mode(db, mode)
                .ok()?
                .homogeneous_element_type(db),
        ))
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
            ty.try_iterate_with_mode(db, EvaluationMode::Sync)
                .ok()?
                .homogeneous_element_type(db),
        ))
    }

    fn infer_star_unpack(
        db: &'db dyn Db,
        ty: Type<'db>,
        prefix: usize,
        suffix: usize,
        position: StarUnpackPosition,
    ) -> Option<ProjectionTerm<'db>> {
        let tuple = ty.try_iterate(db).ok()?;
        Self::project_star_unpack_tuple(db, tuple.as_ref(), prefix, suffix, position)
    }

    fn infer_subscript(
        db: &'db dyn Db,
        ty: Type<'db>,
        subscript: ProjectionSubscript,
    ) -> Option<ProjectionTerm<'db>> {
        Some(ProjectionTerm::Exact(
            ty.subscript(db, subscript.slice_type(db), ast::ExprContext::Load)
                .ok()?,
        ))
    }

    fn infer_context_enter(
        db: &'db dyn Db,
        ty: Type<'db>,
        is_async: bool,
    ) -> Option<ProjectionTerm<'db>> {
        let mode = if is_async {
            EvaluationMode::Async
        } else {
            EvaluationMode::Sync
        };

        Some(ProjectionTerm::Exact(
            ty.try_enter_with_mode(db, mode).ok()?,
        ))
    }

    fn infer_await_result(db: &'db dyn Db, ty: Type<'db>) -> Option<ProjectionTerm<'db>> {
        Some(ProjectionTerm::Exact(ty.try_await(db).ok()?))
    }

    fn into_type(
        self,
        db: &'db dyn Db,
        root: DivergentType,
        solved_ops: &[(ProjectionPath<'db>, Type<'db>)],
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
            Self::Generic {
                class, arguments, ..
            } => {
                let arguments = arguments
                    .into_iter()
                    .map(|argument| {
                        argument.replace_solved_projection_artifacts(db, root, solved_ops)
                    })
                    .collect::<Option<Vec<_>>>()?;

                Type::from(class.apply_specialization(db, |generic_context| {
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
}

/// The result of applying one projection path to one container arm.
#[derive(Debug, Clone, Copy)]
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

    fn is_ambiguous(self, db: &'db dyn Db) -> bool {
        any_over_type(db, self.ty(db), false, |ty| {
            matches!(ty, Type::Dynamic(DynamicType::AmbiguousOverload))
        })
    }
}

/// Projection facts inferred from custom generic containers in a cycle candidate.
#[derive(Debug, Clone)]
struct CycleRecoveryEvidence<'db> {
    arm: Type<'db>,
    path: ProjectionPath<'db>,
    term: ProjectionTerm<'db>,
}

impl<'db> CycleRecoveryEvidence<'db> {
    fn from_type(db: &'db dyn Db, root: DivergentType, ty: Type<'db>) -> Vec<Self> {
        let mut paths = Vec::new();
        Type::collect_projection_ops(db, root, ty, &mut paths);

        let arms = RefCell::new(Vec::new());
        any_over_type(db, ty, false, |nested| {
            if ProjectionContainer::is_custom_generic_container(db, nested) {
                let mut arms = arms.borrow_mut();
                if !arms.contains(&nested) {
                    arms.push(nested);
                }
            }
            false
        });

        let mut evidence: Vec<Self> = Vec::new();
        for arm in arms.into_inner() {
            if arm.same_divergent_marker(db, Type::Divergent(root)) {
                continue;
            }

            for path in &paths {
                for suffix in path.suffixes() {
                    let Some(term) =
                        ProjectionContainer::infer_projection_path(db, arm, suffix.ops())
                    else {
                        continue;
                    };
                    if term.is_ambiguous(db) {
                        continue;
                    }
                    if evidence.iter().any(|existing| {
                        existing.arm == arm
                            && existing.path == suffix
                            && existing.term.ty(db) == term.ty(db)
                    }) {
                        continue;
                    }
                    evidence.push(Self {
                        arm,
                        path: suffix,
                        term,
                    });
                }
            }
        }

        evidence
    }
}

impl ProjectionContainer<'_> {
    fn is_custom_generic_container(db: &dyn Db, ty: Type<'_>) -> bool {
        ty.class_specialization(db)
            .is_some_and(|(class, specialization)| {
                class.known(db).is_none() && !specialization.types(db).is_empty()
            })
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
        (0..self.ops.len()).map(move |index| Self::from_ops(self.ops[index..].iter().copied()))
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

    fn slice_type(self, db: &dyn Db) -> Type<'_> {
        match self {
            Self::Unknown => Type::unknown(),
            Self::Int => KnownClass::Int.to_instance(db),
            Self::LiteralInt(index) => Type::int_literal(index),
            Self::StaticSlice(slice) => slice.into_type(db),
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

impl StaticSliceProjection {
    fn into_type(self, db: &dyn Db) -> Type<'_> {
        let type_for_bound = |bound: Option<i32>| {
            bound.map_or_else(|| Type::none(db), |index| Type::int_literal(index.into()))
        };

        KnownClass::Slice.to_specialized_instance(
            db,
            &[
                type_for_bound(self.start),
                type_for_bound(self.stop),
                type_for_bound(self.step),
            ],
        )
    }
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
