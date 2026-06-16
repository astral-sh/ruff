use std::cell::RefCell;

use super::{
    DivergentType, DynamicType, KnownClass, StaticClassLiteral, TupleSpec, Type, UnionType,
};
use crate::Db;
use crate::types::visitor::any_over_type;

impl<'db> Type<'db> {
    pub(crate) fn try_cycle_iter_projection(self, db: &'db dyn Db) -> Option<Self> {
        self.try_cycle_projection(db, CycleProjectionOp::IterItem)
    }

    pub(crate) fn try_cycle_unpack_projection(
        self,
        db: &'db dyn Db,
        len: usize,
        index: usize,
    ) -> Option<Self> {
        self.try_cycle_projection(db, CycleProjectionOp::UnpackExact { len, index })
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
        let mut containers = Vec::new();
        let mut ops = Vec::new();

        for element in self.top_level_projection_union_elements(db) {
            if element.same_divergent_marker(Type::Divergent(root)) {
                continue;
            }

            let container = ProjectionContainer::from_type(db, element)?;
            container.collect_projection_ops(db, root, &mut ops);
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

        let containers = containers
            .into_iter()
            .map(|container| container.into_type(db, root, &solved_ops))
            .collect::<Option<Vec<_>>>()?;

        Some(UnionType::from_elements_cycle_recovery(db, containers))
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
            }
        }

        if productive_terms.is_empty() {
            return (!saw_self_reference).then_some(Type::Never);
        }

        Some(match productive_terms.as_slice() {
            [term] => *term,
            _ => UnionType::from_elements_cycle_recovery(db, productive_terms),
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

        if let Some(spec) = self.exact_tuple_instance_spec(db) {
            let TupleSpec::Fixed(tuple) = spec.as_ref() else {
                return None;
            };
            let elements = tuple
                .iter_all_elements()
                .map(|element| element.replace_solved_projection_artifacts(db, root, solved_ops))
                .collect::<Option<Vec<_>>>()?;
            return Some(Type::heterogeneous_tuple(db, elements));
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
    FixedTuple {
        elements: Vec<Type<'db>>,
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
            let TupleSpec::Fixed(tuple) = spec.as_ref() else {
                return None;
            };
            return Some(Self::FixedTuple {
                elements: tuple.iter_all_elements().collect(),
            });
        }

        if let Some((class, specialization)) = ty.class_specialization(db) {
            if let Some(known_class) = class.known(db)
                && Self::known_container_iter_item_type(known_class, specialization.types(db))
                    .is_some()
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

    fn collect_projection_ops(
        &self,
        db: &'db dyn Db,
        root: DivergentType,
        paths: &mut Vec<CycleProjectionPath<'db>>,
    ) {
        match self {
            Self::FixedTuple { elements } => {
                for element in elements {
                    Type::collect_projection_ops(db, root, *element, paths);
                }
            }
            Self::Known { arguments, .. } => {
                for argument in arguments {
                    Type::collect_projection_ops(db, root, *argument, paths);
                }
            }
            Self::Custom { arguments, .. } => {
                for argument in arguments {
                    Type::collect_projection_ops(db, root, *argument, paths);
                }
            }
        }
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
            Self::FixedTuple { elements } => {
                Type::heterogeneous_tuple(db, elements.iter().copied())
            }
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
            CycleProjectionOp::UnpackExact { len, index } => {
                Self::project_unpack_exact(db, ty, len, index)?
            }
        };

        if tail.is_empty() {
            return Some(projected);
        }

        Self::project_type_path(
            db,
            projected.ty(),
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
            CycleProjectionOp::UnpackExact { len, index } => {
                Self::infer_unpack_exact(db, ty, len, index)?
            }
        };

        if tail.is_empty() {
            return Some(projected);
        }

        Self::infer_projection_path(db, projected.ty(), tail)
    }

    fn project_iter_item(db: &'db dyn Db, ty: Type<'db>) -> Option<ProjectionTerm<'db>> {
        if let Some(spec) = ty.exact_tuple_instance_spec(db) {
            let TupleSpec::Fixed(tuple) = spec.as_ref() else {
                return None;
            };
            return Some(ProjectionTerm::Homogeneous(
                UnionType::from_elements_cycle_recovery(db, tuple.iter_all_elements()),
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

    fn project_unpack_exact(
        db: &'db dyn Db,
        ty: Type<'db>,
        len: usize,
        index: usize,
    ) -> Option<ProjectionTerm<'db>> {
        if let Some(spec) = ty.exact_tuple_instance_spec(db) {
            let TupleSpec::Fixed(tuple) = spec.as_ref() else {
                return None;
            };
            if tuple.len() != len {
                return None;
            }

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

    fn infer_iter_item(db: &'db dyn Db, ty: Type<'db>) -> Option<ProjectionTerm<'db>> {
        Some(ProjectionTerm::Homogeneous(
            ty.try_iterate(db).ok()?.homogeneous_element_type(db),
        ))
    }

    fn infer_unpack_exact(
        db: &'db dyn Db,
        ty: Type<'db>,
        len: usize,
        index: usize,
    ) -> Option<ProjectionTerm<'db>> {
        if let Some(spec) = ty.exact_tuple_instance_spec(db)
            && let TupleSpec::Fixed(tuple) = spec.as_ref()
            && tuple.len() == len
        {
            return Some(ProjectionTerm::Exact(tuple.iter_all_elements().nth(index)?));
        }

        Some(ProjectionTerm::Homogeneous(
            ty.try_iterate(db).ok()?.homogeneous_element_type(db),
        ))
    }

    fn into_type(
        self,
        db: &'db dyn Db,
        root: DivergentType,
        solved_ops: &[(CycleProjectionPath<'db>, Type<'db>)],
    ) -> Option<Type<'db>> {
        match self {
            Self::FixedTuple { elements } => {
                let elements = elements
                    .into_iter()
                    .map(|element| {
                        element.replace_solved_projection_artifacts(db, root, solved_ops)
                    })
                    .collect::<Option<Vec<_>>>()?;

                Some(Type::heterogeneous_tuple(db, elements))
            }
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
}

#[derive(Debug, Clone, Copy)]
enum ProjectionTerm<'db> {
    Exact(Type<'db>),
    Homogeneous(Type<'db>),
}

impl<'db> ProjectionTerm<'db> {
    const fn ty(self) -> Type<'db> {
        match self {
            ProjectionTerm::Exact(ty) | ProjectionTerm::Homogeneous(ty) => ty,
        }
    }

    fn is_ambiguous(self, db: &'db dyn Db) -> bool {
        any_over_type(db, self.ty(), false, |ty| {
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
                            && existing.term.ty() == term.ty()
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
    UnpackExact { len: usize, index: usize },
}
