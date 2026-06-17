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
    DivergentType, KnownClass, MemberLookupPolicy, TupleSpec, Type, UnionType, call::CallArguments,
    instance::SliceLiteral,
};
use crate::Db;
use crate::place::{DefinedPlace, Definedness, Place};
use crate::subscript::{PyIndex, PySlice};
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

    /// ```python
    /// x: Divergent => x[n]: Projection_{Subscript[n]}(Divergent)
    /// x: Projection_{op}(Divergent) => x[n]: Projection_{op, Subscript[n]}(Divergent)
    /// ```
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

        let mut elements = Vec::new();

        for element in union.elements(db).iter().copied() {
            if let Some(projected) = element.try_projection(db, op) {
                elements.push(projected);
            } else {
                elements.push(project_non_cycle(element)?);
            }
        }

        Some(UnionType::from_elements_cycle_recovery(db, elements))
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
        let mut paths = Vec::new();
        Self::collect_projection_ops(db, root, self, &mut paths);
        if paths.is_empty() {
            return None;
        }

        self.try_container_projection_cycle_normalized(db, root, &paths)
    }

    /// Solves all projections of `root` that can be explained by top-level containers.
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
    ///    `Projection_{Subscript[0]}(D)`. These terms are stored in
    ///    `terms_by_op`.
    ///    * `containers = [tuple[int], tuple[Projection_{Subscript[0]}(D)]]`
    ///    * `terms_by_op = [(Subscript[0], [Exact(int), Exact(Projection_{Subscript[0]}(D))])]`
    /// 3. Solve each path from the terms produced by those containers, dropping
    ///    recursive self references and unioning the remaining productive terms.
    ///    In the example, the self reference is discarded and `Subscript[0]`
    ///    is solved as `int`, producing `solved_ops = [(Subscript[0], int)]`.
    /// 4. Rebuild the original top-level arms with every cycle artifact replaced
    ///    by its solved projection type.
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
    ) -> Option<Self> {
        let elements = self.top_level_projection_union_elements(db);
        let mut containers = Vec::new();

        for element in &elements {
            if element.same_divergent_marker(db, Type::Divergent(root)) {
                continue;
            }

            let container = ProjectionContainer::from_type(db, *element)?;
            containers.push(container);
        }

        if containers.is_empty() {
            return None;
        }

        let mut terms_by_op = paths
            .iter()
            .cloned()
            .map(|path| (path, Vec::new()))
            .collect::<Vec<_>>();
        for container in &containers {
            container.collect_projection_terms(db, &mut terms_by_op)?;
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
                        Some(path),
                        term,
                        &mut saw_self_reference,
                        &mut productive_terms,
                    )?;
                }
                ProjectionTerm::Homogeneous(term) | ProjectionTerm::List(term) => {
                    Self::collect_projection_component_terms(
                        db,
                        root,
                        None,
                        term,
                        &mut saw_self_reference,
                        &mut productive_terms,
                    )?;
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

    fn collect_projection_component_terms(
        db: &'db dyn Db,
        root: DivergentType,
        path: Option<&ProjectionPath<'db>>,
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

        if let Some(path) = path
            && term.mentions_nonmatching_projection(db, root, path)
        {
            return None;
        }

        if term.mentions_cycle_artifact(db, root) {
            *saw_self_reference = true;
            return Some(());
        }

        productive_terms.push(term);
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
    Known {
        class: KnownClass,
        arguments: Vec<Type<'db>>,
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
            if let Some(known_class) = class.known(db)
                && Self::known_container_supports_projection(db, known_class, arguments)
            {
                return Some(Self::Known {
                    class: known_class,
                    arguments: arguments.to_vec(),
                });
            }
        }

        None
    }

    fn known_container_supports_projection(
        db: &'db dyn Db,
        class: KnownClass,
        arguments: &[Type<'db>],
    ) -> bool {
        Self::known_container_iter_item_type(class, arguments, false).is_some()
            || Self::known_container_iter_item_type(class, arguments, true).is_some()
            || Self::known_container_get_item_type(db, class, arguments).is_some()
            || Self::known_container_slice_type(db, class, arguments).is_some()
            || Self::known_container_await_result_type(class, arguments).is_some()
            || Self::known_container_method_call0_type(
                db,
                class,
                arguments,
                &Name::new_static("keys"),
            )
            .is_some()
            || Self::known_container_method_call0_type(
                db,
                class,
                arguments,
                &Name::new_static("values"),
            )
            .is_some()
            || Self::known_container_method_call0_type(
                db,
                class,
                arguments,
                &Name::new_static("items"),
            )
            .is_some()
    }

    fn collect_projection_terms(
        &self,
        db: &'db dyn Db,
        terms_by_op: &mut [(ProjectionPath<'db>, Vec<ProjectionTerm<'db>>)],
    ) -> Option<()> {
        for (path, terms) in terms_by_op {
            terms.push(self.project_path(db, path)?);
        }
        Some(())
    }

    fn project_path(
        &self,
        db: &'db dyn Db,
        path: &ProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        let ty = match self {
            Self::Tuple { spec } => Type::tuple(TupleType::new(db, spec)),
            Self::Known { class, arguments } => class.to_specialized_instance(db, arguments),
        };
        Self::project_type_path(db, ty, path)
    }

    fn project_type_path(
        db: &'db dyn Db,
        ty: Type<'db>,
        path: &ProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        let ops = path.ops();
        let (&op, tail) = ops.split_first()?;

        let projected = Self::project_op(db, ty, op)?;

        if tail.is_empty() {
            return Some(projected);
        }

        Self::project_type_path(
            db,
            projected.ty(db),
            &ProjectionPath::from_ops(tail.iter().copied()),
        )
    }

    fn project_op(
        db: &'db dyn Db,
        ty: Type<'db>,
        op: ProjectionOp<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        match op {
            ProjectionOp::Iter { is_async } => Self::project_iter_item(db, ty, is_async),
            ProjectionOp::Unpack(unpack) => Self::project_unpack(db, ty, unpack),
            ProjectionOp::Subscript(subscript) => Self::project_subscript(db, ty, subscript),
            ProjectionOp::CallMethod0(method) => Self::project_method_call0(db, ty, method),
            ProjectionOp::ContextEnter { .. } => None,
            ProjectionOp::AwaitResult => Self::project_await_result(db, ty),
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

        if let Some((class, specialization)) = ty.class_specialization(db)
            && let Some(known_class) = class.known(db)
            && let Some(element) = Self::known_container_iter_item_type(
                known_class,
                specialization.types(db),
                is_async,
            )
        {
            return Some(ProjectionTerm::Homogeneous(element));
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

        if let Some((class, specialization)) = ty.class_specialization(db)
            && let Some(known_class) = class.known(db)
            && let Some(element) =
                Self::known_container_iter_item_type(known_class, specialization.types(db), false)
        {
            return Some(ProjectionTerm::Homogeneous(element));
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

        if let Some((class, specialization)) = ty.class_specialization(db)
            && let Some(known_class) = class.known(db)
            && let Some(element) =
                Self::known_container_iter_item_type(known_class, specialization.types(db), false)
        {
            return Some(Self::star_unpack_homogeneous(element, position));
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

        if let Some((class, specialization)) = ty.class_specialization(db)
            && let Some(known_class) = class.known(db)
        {
            return match subscript {
                ProjectionSubscript::LiteralInt(_)
                | ProjectionSubscript::Int
                | ProjectionSubscript::Unknown => {
                    Self::known_container_get_item_type(db, known_class, specialization.types(db))
                        .map(ProjectionTerm::Homogeneous)
                }
                ProjectionSubscript::StaticSlice(_) => {
                    Self::known_container_slice_type(db, known_class, specialization.types(db))
                        .map(ProjectionTerm::Exact)
                }
            };
        }

        None
    }

    fn project_method_call0(
        db: &'db dyn Db,
        ty: Type<'db>,
        method: ProjectionMethodName<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        if let Some((class, specialization)) = ty.class_specialization(db)
            && let Some(known_class) = class.known(db)
            && let Some(return_ty) = Self::known_container_method_call0_type(
                db,
                known_class,
                specialization.types(db),
                method.name(db),
            )
        {
            return Some(ProjectionTerm::Exact(return_ty));
        }

        None
    }

    fn project_await_result(db: &'db dyn Db, ty: Type<'db>) -> Option<ProjectionTerm<'db>> {
        if let Some((class, specialization)) = ty.class_specialization(db)
            && let Some(known_class) = class.known(db)
            && let Some(result) =
                Self::known_container_await_result_type(known_class, specialization.types(db))
        {
            return Some(ProjectionTerm::Exact(result));
        }

        None
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
            Self::Known { class, arguments } => {
                let arguments = arguments
                    .into_iter()
                    .map(|argument| {
                        argument.replace_solved_projection_artifacts(db, root, solved_ops)
                    })
                    .collect::<Option<Vec<_>>>()?;

                Some(class.to_specialized_instance(db, &arguments))
            }
        }
    }

    fn known_container_iter_item_type(
        class: KnownClass,
        arguments: &[Type<'db>],
        is_async: bool,
    ) -> Option<Type<'db>> {
        let index = if is_async {
            match class {
                KnownClass::AsyncGenerator
                | KnownClass::AsyncGeneratorType
                | KnownClass::AsyncIterator
                | KnownClass::TyExtensionsAsyncIterable
                | KnownClass::TyExtensionsAsyncIterator => 0,
                _ => return None,
            }
        } else {
            match class {
                KnownClass::List
                | KnownClass::Set
                | KnownClass::FrozenSet
                | KnownClass::Deque
                | KnownClass::Iterable
                | KnownClass::Iterator
                | KnownClass::Sequence
                | KnownClass::TyExtensionsIterable
                | KnownClass::TyExtensionsIterator => 0,

                KnownClass::Dict
                | KnownClass::DefaultDict
                | KnownClass::OrderedDict
                | KnownClass::ChainMap
                | KnownClass::Counter
                | KnownClass::Mapping => 0,

                KnownClass::Generator => 0,

                _ => return None,
            }
        };

        arguments.get(index).copied()
    }

    fn known_container_await_result_type(
        class: KnownClass,
        arguments: &[Type<'db>],
    ) -> Option<Type<'db>> {
        let index = match class {
            KnownClass::Awaitable => 0,
            KnownClass::CoroutineType => 2,
            _ => return None,
        };

        arguments.get(index).copied()
    }

    fn known_container_get_item_type(
        db: &'db dyn Db,
        class: KnownClass,
        arguments: &[Type<'db>],
    ) -> Option<Type<'db>> {
        match class {
            KnownClass::List | KnownClass::Deque | KnownClass::Sequence => {
                arguments.first().copied()
            }
            KnownClass::Dict
            | KnownClass::DefaultDict
            | KnownClass::OrderedDict
            | KnownClass::ChainMap
            | KnownClass::Mapping => arguments.get(1).copied(),
            KnownClass::Counter => Some(KnownClass::Int.to_instance(db)),
            _ => None,
        }
    }

    fn known_container_slice_type(
        db: &'db dyn Db,
        class: KnownClass,
        arguments: &[Type<'db>],
    ) -> Option<Type<'db>> {
        let element = match class {
            KnownClass::List | KnownClass::Sequence => arguments.first().copied()?,
            _ => return None,
        };

        Some(class.to_specialized_instance(db, &[element]))
    }

    fn known_container_method_call0_type(
        db: &'db dyn Db,
        class: KnownClass,
        arguments: &[Type<'db>],
        method_name: &Name,
    ) -> Option<Type<'db>> {
        let item = Self::known_container_mapping_view_item_type(db, class, arguments, method_name)?;
        Some(KnownClass::Iterable.to_specialized_instance(db, &[item]))
    }

    fn known_container_mapping_view_item_type(
        db: &'db dyn Db,
        class: KnownClass,
        arguments: &[Type<'db>],
        method_name: &Name,
    ) -> Option<Type<'db>> {
        let key = arguments.first().copied()?;
        let value = match class {
            KnownClass::Dict
            | KnownClass::DefaultDict
            | KnownClass::OrderedDict
            | KnownClass::ChainMap
            | KnownClass::Mapping => arguments.get(1).copied()?,
            KnownClass::Counter => KnownClass::Int.to_instance(db),
            _ => return None,
        };

        match method_name.as_str() {
            "keys" => Some(key),
            "values" => Some(value),
            "items" => Some(Type::heterogeneous_tuple(db, [key, value])),
            _ => None,
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
