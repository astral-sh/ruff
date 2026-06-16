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
use crate::subscript::{PyIndex, PySlice};
use crate::types::tuple::{Tuple, TupleLength, TupleType};
use crate::types::visitor::any_over_type;

impl<'db> Type<'db> {
    pub(crate) fn try_cycle_iter_projection_with_mode(
        self,
        db: &'db dyn Db,
        mode: EvaluationMode,
    ) -> Option<Self> {
        let op = if mode.is_async() {
            CycleProjectionOp::AsyncIterItem
        } else {
            CycleProjectionOp::IterItem
        };
        self.try_cycle_projection_with_non_cycle(db, op, |ty| {
            ty.try_iterate_with_mode(db, mode)
                .ok()
                .map(|tuple| tuple.homogeneous_element_type(db))
        })
    }

    pub(crate) fn try_cycle_unpack_projection(
        self,
        db: &'db dyn Db,
        len: usize,
        index: usize,
    ) -> Option<Self> {
        self.try_cycle_projection(db, CycleProjectionOp::UnpackExact { len, index })
    }

    pub(crate) fn try_cycle_star_unpack_prefix_projection(
        self,
        db: &'db dyn Db,
        prefix: usize,
        suffix: usize,
        index: usize,
    ) -> Option<Self> {
        self.try_cycle_projection(
            db,
            CycleProjectionOp::UnpackStarPrefix {
                prefix,
                suffix,
                index,
            },
        )
    }

    pub(crate) fn try_cycle_star_unpack_rest_projection(
        self,
        db: &'db dyn Db,
        prefix: usize,
        suffix: usize,
    ) -> Option<Self> {
        self.try_cycle_projection(db, CycleProjectionOp::UnpackStarRest { prefix, suffix })
    }

    pub(crate) fn try_cycle_star_unpack_suffix_projection(
        self,
        db: &'db dyn Db,
        prefix: usize,
        suffix: usize,
        index: usize,
    ) -> Option<Self> {
        self.try_cycle_projection(
            db,
            CycleProjectionOp::UnpackStarSuffix {
                prefix,
                suffix,
                index,
            },
        )
    }

    pub(crate) fn try_cycle_subscript_projection(
        self,
        db: &'db dyn Db,
        slice_ty: Type<'db>,
    ) -> Option<Self> {
        let op = CycleProjectionOp::from_subscript(db, slice_ty).or_else(|| {
            (!slice_ty.is_instance_of(db, KnownClass::Slice)).then_some(CycleProjectionOp::GetItem)
        })?;
        self.try_cycle_projection_with_non_cycle(db, op, |ty| {
            ty.subscript(db, slice_ty, ast::ExprContext::Load).ok()
        })
    }

    pub(crate) fn try_cycle_mapping_view_projection(
        self,
        db: &'db dyn Db,
        method_name: &str,
    ) -> Option<Self> {
        let op = CycleProjectionOp::from_mapping_view_method(method_name)?;
        let item_ty = self.try_cycle_projection_with_non_cycle(db, op, |ty| {
            ProjectionContainer::mapping_view_item_type_for_type(db, ty, op)
        })?;

        Some(KnownClass::Iterable.to_specialized_instance(db, &[item_ty]))
    }

    pub(crate) fn try_cycle_context_enter_projection(
        self,
        db: &'db dyn Db,
        mode: EvaluationMode,
    ) -> Option<Self> {
        let op = CycleProjectionOp::ContextEnter {
            is_async: mode.is_async(),
        };
        self.try_cycle_projection_with_non_cycle(db, op, |ty| ty.try_enter_with_mode(db, mode).ok())
    }

    pub(crate) fn try_cycle_await_projection(self, db: &'db dyn Db) -> Option<Self> {
        self.try_cycle_projection_with_non_cycle(db, CycleProjectionOp::AwaitResult, |ty| {
            ty.try_await(db).ok()
        })
    }

    fn try_cycle_projection(self, db: &'db dyn Db, op: CycleProjectionOp) -> Option<Self> {
        match self {
            Type::Divergent(root) => Some(Self::CycleProjection(CycleProjectionType {
                root,
                path: CycleProjectionPath::from_op(db, op),
            })),
            Type::CycleProjection(projection) => {
                Some(Self::CycleProjection(projection.append(db, op)))
            }
            _ => None,
        }
    }

    fn try_cycle_projection_with_non_cycle(
        self,
        db: &'db dyn Db,
        op: CycleProjectionOp,
        mut project_non_cycle: impl FnMut(Self) -> Option<Self>,
    ) -> Option<Self> {
        let Type::Union(union) = self else {
            return self.try_cycle_projection(db, op);
        };

        let mut saw_cycle_projection = false;
        let mut elements = Vec::new();

        for element in union.elements(db).iter().copied() {
            if let Some(projected) = element.try_cycle_projection(db, op) {
                saw_cycle_projection = true;
                elements.push(projected);
            } else {
                elements.push(project_non_cycle(element)?);
            }
        }

        saw_cycle_projection.then(|| UnionType::from_elements_cycle_recovery(db, elements))
    }

    pub(crate) const fn is_cycle_artifact(&self) -> bool {
        matches!(self, Type::Divergent(_) | Type::CycleProjection(_))
    }

    /// Returns `true` if both types originate from the same cycle root, regardless
    /// of whether either occurrence is a direct marker or a projection of it.
    pub(crate) fn same_divergent_marker(self, other: Type<'db>) -> bool {
        match (self, other) {
            (Type::Divergent(left), Type::Divergent(right)) => left.same_marker(right),
            (Type::CycleProjection(left), Type::Divergent(right))
            | (Type::Divergent(right), Type::CycleProjection(left)) => left.root.same_marker(right),
            (Type::CycleProjection(left), Type::CycleProjection(right)) => {
                left.root.same_marker(right.root)
            }
            _ => false,
        }
    }

    pub(crate) fn try_projection_cycle_normalized(
        self,
        db: &'db dyn Db,
        root: DivergentType,
    ) -> Option<Self> {
        if !self.contains_cycle_projection_for_root(db, root) {
            return None;
        }

        let evidence = CycleRecoveryEvidence::from_type(db, root, self);
        self.try_container_projection_cycle_normalized(db, root, &evidence)
    }

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

            if element.same_divergent_marker(Type::Divergent(root)) {
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
            .copied()
            .map(|op| (op, Vec::new()))
            .collect::<Vec<_>>();
        for container in &containers {
            container.collect_projection_terms(db, root, evidence, &mut terms_by_op)?;
        }

        let solved_ops = terms_by_op
            .iter()
            .map(|(op, terms)| Some((*op, Self::solve_projection_terms(db, root, *op, terms)?)))
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

    fn solve_projection_terms(
        db: &'db dyn Db,
        root: DivergentType,
        path: CycleProjectionPath<'db>,
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
        path: CycleProjectionPath<'db>,
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

        if term.mentions_nonmatching_cycle_projection(db, root, path) {
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
        paths: &mut Vec<CycleProjectionPath<'db>>,
    ) {
        let paths = RefCell::new(paths);
        any_over_type(db, ty, false, |nested| {
            if let Type::CycleProjection(projection) = nested
                && projection.root().same_marker(root)
            {
                let mut paths = paths.borrow_mut();
                if !paths.contains(&projection.path()) {
                    paths.push(projection.path());
                }
            }
            false
        });
    }

    fn solved_projection_type(
        solved_ops: &[(CycleProjectionPath<'db>, Type<'db>)],
        path: CycleProjectionPath<'db>,
    ) -> Option<Self> {
        solved_ops
            .iter()
            .find_map(|(candidate, ty)| (*candidate == path).then_some(*ty))
    }

    fn replace_solved_projection_artifacts(
        self,
        db: &'db dyn Db,
        root: DivergentType,
        solved_ops: &[(CycleProjectionPath<'db>, Type<'db>)],
    ) -> Option<Self> {
        if !self.mentions_cycle_artifact(db, root) {
            return Some(self);
        }

        if let Type::CycleProjection(projection) = self
            && projection.root().same_marker(root)
        {
            return Self::solved_projection_type(solved_ops, projection.path());
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
            [path] => Self::solved_projection_type(solved_ops, *path),
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

    fn contains_cycle_projection_for_root(self, db: &'db dyn Db, root: DivergentType) -> bool {
        any_over_type(db, self, false, |ty| match ty {
            Type::CycleProjection(projection) => projection.root().same_marker(root),
            _ => false,
        })
    }

    fn mentions_cycle_artifact(self, db: &'db dyn Db, root: DivergentType) -> bool {
        any_over_type(db, self, false, |ty| match ty {
            Type::Divergent(divergent) => divergent.same_marker(root),
            Type::CycleProjection(projection) => projection.root().same_marker(root),
            _ => false,
        })
    }

    fn mentions_nonmatching_cycle_projection(
        self,
        db: &'db dyn Db,
        root: DivergentType,
        path: CycleProjectionPath<'db>,
    ) -> bool {
        any_over_type(db, self, false, |ty| match ty {
            Type::CycleProjection(projection) => {
                projection.root().same_marker(root) && projection.path() != path
            }
            _ => false,
        })
    }
}

#[derive(Debug, Clone)]
enum ProjectionContainer<'db> {
    Tuple {
        spec: TupleSpec<'db>,
    },
    Known {
        class: KnownClass,
        arguments: Vec<Type<'db>>,
    },
    Custom {
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
            if let Some(known_class) = class.known(db)
                && Self::known_container_supports_projection(
                    db,
                    known_class,
                    specialization.types(db),
                )
            {
                return Some(Self::Known {
                    class: known_class,
                    arguments: specialization.types(db).to_vec(),
                });
            }

            if class.known(db).is_none() && !specialization.types(db).is_empty() {
                return Some(Self::Custom {
                    class,
                    arguments: specialization.types(db).to_vec(),
                    arm: ty,
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
        Self::known_container_iter_item_type(class, arguments).is_some()
            || Self::known_container_async_iter_item_type(class, arguments).is_some()
            || Self::known_container_get_item_type(db, class, arguments).is_some()
            || Self::known_container_slice_type_for_class(class, arguments).is_some()
            || Self::known_container_await_result_type(class, arguments).is_some()
            || Self::known_container_mapping_view_item_type(
                db,
                class,
                arguments,
                CycleProjectionOp::MappingKeys,
            )
            .is_some()
            || Self::known_container_mapping_view_item_type(
                db,
                class,
                arguments,
                CycleProjectionOp::MappingValues,
            )
            .is_some()
            || Self::known_container_mapping_view_item_type(
                db,
                class,
                arguments,
                CycleProjectionOp::MappingItems,
            )
            .is_some()
    }

    fn collect_projection_terms(
        &self,
        db: &'db dyn Db,
        root: DivergentType,
        evidence: &[CycleRecoveryEvidence<'db>],
        terms_by_op: &mut [(CycleProjectionPath<'db>, Vec<ProjectionTerm<'db>>)],
    ) -> Option<()> {
        for (path, terms) in terms_by_op {
            terms.push(self.project_path(db, root, evidence, *path)?);
        }
        Some(())
    }

    fn project_path(
        &self,
        db: &'db dyn Db,
        root: DivergentType,
        evidence: &[CycleRecoveryEvidence<'db>],
        path: CycleProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        let ty = match self {
            Self::Tuple { spec } => Type::tuple(TupleType::new(db, spec)),
            Self::Known { class, arguments } => class.to_specialized_instance(db, arguments),
            Self::Custom { arm, .. } => {
                return Self::project_custom_path(db, *arm, root, evidence, path);
            }
        };
        Self::project_type_path(db, ty, root, evidence, path)
    }

    fn project_type_path(
        db: &'db dyn Db,
        ty: Type<'db>,
        root: DivergentType,
        evidence: &[CycleRecoveryEvidence<'db>],
        path: CycleProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        if let Some(term) = Self::project_custom_path(db, ty, root, evidence, path) {
            return Some(term);
        }

        let ops = path.ops(db);
        let (&op, tail) = ops.split_first()?;

        let projected = match op {
            CycleProjectionOp::IterItem => Self::project_iter_item(db, ty)?,
            CycleProjectionOp::AsyncIterItem => Self::project_async_iter_item(db, ty)?,
            CycleProjectionOp::UnpackExact { len, index } => {
                Self::project_unpack_exact(db, ty, len, index)?
            }
            CycleProjectionOp::UnpackStarPrefix {
                prefix,
                suffix,
                index,
            } => Self::project_star_unpack(
                db,
                ty,
                prefix,
                suffix,
                StarUnpackPosition::Prefix(index),
            )?,
            CycleProjectionOp::UnpackStarRest { prefix, suffix } => {
                Self::project_star_unpack(db, ty, prefix, suffix, StarUnpackPosition::Rest)?
            }
            CycleProjectionOp::UnpackStarSuffix {
                prefix,
                suffix,
                index,
            } => Self::project_star_unpack(
                db,
                ty,
                prefix,
                suffix,
                StarUnpackPosition::Suffix(index),
            )?,
            CycleProjectionOp::GetItemLiteralInt(index) => {
                Self::project_get_item_int(db, ty, Some(index))?
            }
            CycleProjectionOp::GetItemInt => Self::project_get_item_int(db, ty, None)?,
            CycleProjectionOp::GetItem => Self::project_get_item(db, ty)?,
            CycleProjectionOp::SliceStatic(slice) => Self::project_slice_static(db, ty, slice)?,
            CycleProjectionOp::MappingKeys
            | CycleProjectionOp::MappingValues
            | CycleProjectionOp::MappingItems => Self::project_mapping_view_item(db, ty, op)?,
            CycleProjectionOp::ContextEnter { .. } => {
                return None;
            }
            CycleProjectionOp::AwaitResult => Self::project_await_result(db, ty)?,
        };

        if tail.is_empty() {
            return Some(projected);
        }

        Self::project_type_path(
            db,
            projected.ty(db),
            root,
            evidence,
            CycleProjectionPath::from_ops(db, tail.iter().copied()),
        )
    }

    fn project_custom_path(
        db: &'db dyn Db,
        ty: Type<'db>,
        root: DivergentType,
        evidence: &[CycleRecoveryEvidence<'db>],
        path: CycleProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        Self::is_custom_generic_container(db, ty).then_some(())?;
        evidence.iter().find_map(|fact| {
            (fact.root.same_marker(root) && fact.arm == ty && fact.path == path)
                .then_some(fact.term)
        })
    }

    fn infer_projection_path(
        db: &'db dyn Db,
        ty: Type<'db>,
        ops: &[CycleProjectionOp],
    ) -> Option<ProjectionTerm<'db>> {
        let (&op, tail) = ops.split_first()?;

        let projected = match op {
            CycleProjectionOp::IterItem => Self::infer_iter_item(db, ty)?,
            CycleProjectionOp::AsyncIterItem => Self::infer_async_iter_item(db, ty)?,
            CycleProjectionOp::UnpackExact { len, index } => {
                Self::infer_unpack_exact(db, ty, len, index)?
            }
            CycleProjectionOp::UnpackStarPrefix {
                prefix,
                suffix,
                index,
            } => {
                Self::infer_star_unpack(db, ty, prefix, suffix, StarUnpackPosition::Prefix(index))?
            }
            CycleProjectionOp::UnpackStarRest { prefix, suffix } => {
                Self::infer_star_unpack(db, ty, prefix, suffix, StarUnpackPosition::Rest)?
            }
            CycleProjectionOp::UnpackStarSuffix {
                prefix,
                suffix,
                index,
            } => {
                Self::infer_star_unpack(db, ty, prefix, suffix, StarUnpackPosition::Suffix(index))?
            }
            CycleProjectionOp::GetItemLiteralInt(index) => {
                Self::infer_subscript(db, ty, Type::int_literal(index))?
            }
            CycleProjectionOp::GetItemInt => {
                Self::infer_subscript(db, ty, KnownClass::Int.to_instance(db))?
            }
            CycleProjectionOp::GetItem => Self::infer_subscript(db, ty, Type::unknown())?,
            CycleProjectionOp::SliceStatic(slice) => {
                Self::infer_subscript(db, ty, slice.into_type(db))?
            }
            CycleProjectionOp::MappingKeys
            | CycleProjectionOp::MappingValues
            | CycleProjectionOp::MappingItems => Self::project_mapping_view_item(db, ty, op)?,
            CycleProjectionOp::ContextEnter { is_async } => {
                Self::infer_context_enter(db, ty, is_async)?
            }
            CycleProjectionOp::AwaitResult => Self::infer_await_result(db, ty)?,
        };

        if tail.is_empty() {
            return Some(projected);
        }

        Self::infer_projection_path(db, projected.ty(db), tail)
    }

    fn project_iter_item(db: &'db dyn Db, ty: Type<'db>) -> Option<ProjectionTerm<'db>> {
        if let Some(spec) = ty.exact_tuple_instance_spec(db) {
            return Some(ProjectionTerm::Homogeneous(
                spec.as_ref().homogeneous_element_type(db),
            ));
        }

        if let Some((class, specialization)) = ty.class_specialization(db)
            && let Some(known_class) = class.known(db)
            && let Some(element) =
                Self::known_container_iter_item_type(known_class, specialization.types(db))
        {
            return Some(ProjectionTerm::Homogeneous(element));
        }

        None
    }

    fn project_async_iter_item(db: &'db dyn Db, ty: Type<'db>) -> Option<ProjectionTerm<'db>> {
        if let Some((class, specialization)) = ty.class_specialization(db)
            && let Some(known_class) = class.known(db)
            && let Some(element) =
                Self::known_container_async_iter_item_type(known_class, specialization.types(db))
        {
            return Some(ProjectionTerm::Homogeneous(element));
        }

        None
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
                Self::known_container_iter_item_type(known_class, specialization.types(db))
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
                Self::known_container_iter_item_type(known_class, specialization.types(db))
        {
            return Some(Self::star_unpack_homogeneous(element, position));
        }

        None
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

    fn project_get_item_int(
        db: &'db dyn Db,
        ty: Type<'db>,
        index: Option<i64>,
    ) -> Option<ProjectionTerm<'db>> {
        if let Some(spec) = ty.exact_tuple_instance_spec(db) {
            let tuple = spec.as_ref();

            if let Some(index) = index {
                let index = i32::try_from(index).ok()?;
                return Some(ProjectionTerm::Exact(tuple.py_index(db, index).ok()?));
            }

            return Some(ProjectionTerm::Homogeneous(
                tuple.homogeneous_element_type(db),
            ));
        }

        if let Some((class, specialization)) = ty.class_specialization(db)
            && let Some(known_class) = class.known(db)
            && let Some(element) =
                Self::known_container_get_item_type(db, known_class, specialization.types(db))
        {
            return Some(ProjectionTerm::Homogeneous(element));
        }

        None
    }

    fn project_get_item(db: &'db dyn Db, ty: Type<'db>) -> Option<ProjectionTerm<'db>> {
        if let Some(spec) = ty.exact_tuple_instance_spec(db) {
            return Some(ProjectionTerm::Homogeneous(
                spec.as_ref().homogeneous_element_type(db),
            ));
        }

        if let Some((class, specialization)) = ty.class_specialization(db)
            && let Some(known_class) = class.known(db)
            && let Some(element) =
                Self::known_container_get_item_type(db, known_class, specialization.types(db))
        {
            return Some(ProjectionTerm::Homogeneous(element));
        }

        None
    }

    fn project_slice_static(
        db: &'db dyn Db,
        ty: Type<'db>,
        slice: StaticSliceProjection,
    ) -> Option<ProjectionTerm<'db>> {
        if let Some(spec) = ty.exact_tuple_instance_spec(db) {
            return match spec.as_ref() {
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
            };
        }

        if let Some((class, specialization)) = ty.class_specialization(db)
            && let Some(known_class) = class.known(db)
            && let Some(sliced) =
                Self::known_container_slice_type(db, known_class, specialization.types(db))
        {
            return Some(ProjectionTerm::Exact(sliced));
        }

        None
    }

    fn project_mapping_view_item(
        db: &'db dyn Db,
        ty: Type<'db>,
        view: CycleProjectionOp,
    ) -> Option<ProjectionTerm<'db>> {
        Some(ProjectionTerm::Exact(
            Self::mapping_view_item_type_for_type(db, ty, view)?,
        ))
    }

    fn infer_iter_item(db: &'db dyn Db, ty: Type<'db>) -> Option<ProjectionTerm<'db>> {
        Some(ProjectionTerm::Homogeneous(
            ty.try_iterate(db).ok()?.homogeneous_element_type(db),
        ))
    }

    fn infer_async_iter_item(db: &'db dyn Db, ty: Type<'db>) -> Option<ProjectionTerm<'db>> {
        Some(ProjectionTerm::Homogeneous(
            ty.try_iterate_with_mode(db, EvaluationMode::Async)
                .ok()?
                .homogeneous_element_type(db),
        ))
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
        let tuple = ty.try_iterate(db).ok()?;
        Self::project_star_unpack_tuple(db, tuple.as_ref(), prefix, suffix, position)
    }

    fn infer_subscript(
        db: &'db dyn Db,
        ty: Type<'db>,
        slice_ty: Type<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        Some(ProjectionTerm::Exact(
            ty.subscript(db, slice_ty, ast::ExprContext::Load).ok()?,
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
        solved_ops: &[(CycleProjectionPath<'db>, Type<'db>)],
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
                Some(class.to_specialized_instance(db, arguments))
            }
            Self::Custom {
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

    fn known_container_iter_item_type(
        class: KnownClass,
        arguments: &[Type<'db>],
    ) -> Option<Type<'db>> {
        // Keep this to sync iteration; `CycleProjectionOp` does not record the iteration mode.
        let index = match class {
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
        };

        arguments.get(index).copied()
    }

    fn known_container_async_iter_item_type(
        class: KnownClass,
        arguments: &[Type<'db>],
    ) -> Option<Type<'db>> {
        let index = match class {
            KnownClass::AsyncGenerator
            | KnownClass::AsyncGeneratorType
            | KnownClass::AsyncIterator
            | KnownClass::TyExtensionsAsyncIterable
            | KnownClass::TyExtensionsAsyncIterator => 0,
            _ => return None,
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

    fn known_container_slice_type_for_class(
        class: KnownClass,
        arguments: &[Type<'db>],
    ) -> Option<()> {
        match class {
            KnownClass::List | KnownClass::Sequence => arguments.first().map(|_| ()),
            _ => None,
        }
    }

    fn mapping_view_item_type_for_type(
        db: &'db dyn Db,
        ty: Type<'db>,
        view: CycleProjectionOp,
    ) -> Option<Type<'db>> {
        if let Some(item) = Self::known_mapping_view_item_type_for_type(db, ty, view) {
            return Some(item);
        }

        Self::inferred_mapping_view_item_type_for_type(db, ty, view)
    }

    fn known_mapping_view_item_type_for_type(
        db: &'db dyn Db,
        ty: Type<'db>,
        view: CycleProjectionOp,
    ) -> Option<Type<'db>> {
        let (class, specialization) = ty.class_specialization(db)?;
        Self::known_container_mapping_view_item_type(
            db,
            class.known(db)?,
            specialization.types(db),
            view,
        )
    }

    fn known_container_mapping_view_item_type(
        db: &'db dyn Db,
        class: KnownClass,
        arguments: &[Type<'db>],
        view: CycleProjectionOp,
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

        match view {
            CycleProjectionOp::MappingKeys => Some(key),
            CycleProjectionOp::MappingValues => Some(value),
            CycleProjectionOp::MappingItems => Some(Type::heterogeneous_tuple(db, [key, value])),
            _ => None,
        }
    }

    fn inferred_mapping_view_item_type_for_type(
        db: &'db dyn Db,
        ty: Type<'db>,
        view: CycleProjectionOp,
    ) -> Option<Type<'db>> {
        let method_name = view.mapping_view_method_name()?;
        let Place::Defined(DefinedPlace {
            ty: method,
            definedness: Definedness::AlwaysDefined,
            ..
        }) = ty
            .member_lookup_with_policy(
                db,
                Name::new_static(method_name),
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
                .return_type(db)
                .try_iterate(db)
                .ok()?
                .homogeneous_element_type(db),
        )
    }
}

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

/// Projection facts for custom generic containers in the current cycle candidate.
#[derive(Debug, Clone, Copy)]
struct CycleRecoveryEvidence<'db> {
    root: DivergentType,
    arm: Type<'db>,
    path: CycleProjectionPath<'db>,
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
            if arm.same_divergent_marker(Type::Divergent(root)) {
                continue;
            }

            for path in &paths {
                for suffix in path.suffixes(db) {
                    let Some(term) =
                        ProjectionContainer::infer_projection_path(db, arm, suffix.ops(db))
                    else {
                        continue;
                    };
                    if term.is_ambiguous(db) {
                        continue;
                    }
                    if evidence.iter().any(|existing| {
                        existing.root.same_marker(root)
                            && existing.arm == arm
                            && existing.path == suffix
                            && existing.term.ty(db) == term.ty(db)
                    }) {
                        continue;
                    }
                    evidence.push(Self {
                        root,
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

/// A query-free projection of a cycle root produced while recovering recursive inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub struct CycleProjectionType<'db> {
    root: DivergentType,
    path: CycleProjectionPath<'db>,
}

impl<'db> CycleProjectionType<'db> {
    pub(crate) const fn root(self) -> DivergentType {
        self.root
    }

    const fn path(self) -> CycleProjectionPath<'db> {
        self.path
    }

    fn append(self, db: &'db dyn Db, op: CycleProjectionOp) -> Self {
        Self {
            root: self.root,
            path: self.path.append(db, op),
        }
    }
}

#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
struct CycleProjectionPath<'db> {
    #[returns(deref)]
    ops: Box<[CycleProjectionOp]>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for CycleProjectionPath<'_> {}

impl<'db> CycleProjectionPath<'db> {
    fn from_op(db: &'db dyn Db, op: CycleProjectionOp) -> Self {
        Self::from_ops(db, [op])
    }

    fn from_ops(db: &'db dyn Db, ops: impl IntoIterator<Item = CycleProjectionOp>) -> Self {
        Self::new_internal(db, ops.into_iter().collect::<Vec<_>>().into_boxed_slice())
    }

    fn append(self, db: &'db dyn Db, op: CycleProjectionOp) -> Self {
        let mut ops = self.ops(db).to_vec();
        ops.push(op);
        Self::new_internal(db, ops.into_boxed_slice())
    }

    fn suffixes(self, db: &'db dyn Db) -> impl Iterator<Item = Self> + 'db {
        (0..self.ops(db).len())
            .map(move |index| Self::from_ops(db, self.ops(db)[index..].iter().copied()))
    }
}

/// The projection operations currently preserved through cycle recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) enum CycleProjectionOp {
    IterItem,
    AsyncIterItem,
    UnpackExact {
        len: usize,
        index: usize,
    },
    UnpackStarPrefix {
        prefix: usize,
        suffix: usize,
        index: usize,
    },
    UnpackStarRest {
        prefix: usize,
        suffix: usize,
    },
    UnpackStarSuffix {
        prefix: usize,
        suffix: usize,
        index: usize,
    },
    GetItem,
    GetItemLiteralInt(i64),
    GetItemInt,
    SliceStatic(StaticSliceProjection),
    MappingKeys,
    MappingValues,
    MappingItems,
    ContextEnter {
        is_async: bool,
    },
    AwaitResult,
}

impl<'db> CycleProjectionOp {
    fn from_mapping_view_method(method_name: &str) -> Option<Self> {
        match method_name {
            "keys" => Some(Self::MappingKeys),
            "values" => Some(Self::MappingValues),
            "items" => Some(Self::MappingItems),
            _ => None,
        }
    }

    const fn mapping_view_method_name(self) -> Option<&'static str> {
        match self {
            Self::MappingKeys => Some("keys"),
            Self::MappingValues => Some("values"),
            Self::MappingItems => Some("items"),
            _ => None,
        }
    }

    fn from_subscript(db: &'db dyn Db, slice_ty: Type<'db>) -> Option<Self> {
        if let Some(index) = slice_ty.as_int_like_literal() {
            return Some(Self::GetItemLiteralInt(index));
        }

        if let Some(slice) = slice_ty
            .as_nominal_instance()
            .and_then(|instance| instance.slice_literal(db))
            && slice.step != Some(0)
        {
            return Some(Self::SliceStatic(StaticSliceProjection::from(slice)));
        }

        if slice_ty.is_instance_of(db, KnownClass::Int)
            || slice_ty.is_instance_of(db, KnownClass::Bool)
        {
            return Some(Self::GetItemInt);
        }

        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StarUnpackPosition {
    Prefix(usize),
    Rest,
    Suffix(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) struct StaticSliceProjection {
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
