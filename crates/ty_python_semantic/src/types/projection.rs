use std::cell::RefCell;

use super::{DivergentType, KnownClass, TupleSpec, Type, UnionType};
use crate::Db;
use crate::types::visitor::any_over_type;

impl<'db> Type<'db> {
    pub(crate) const fn cycle_unpack_projection(
        root: DivergentType,
        len: usize,
        index: usize,
    ) -> Self {
        Self::CycleProjection(CycleProjectionType {
            root,
            op: CycleProjectionOp::UnpackExact { len, index },
        })
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

        self.try_container_projection_cycle_normalized(db, root)
    }

    fn try_container_projection_cycle_normalized(
        self,
        db: &'db dyn Db,
        root: DivergentType,
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
            container.collect_projection_terms(&mut terms_by_op);
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
        op: CycleProjectionOp,
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
                        op,
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
        op: CycleProjectionOp,
        term: Type<'db>,
        saw_self_reference: &mut bool,
        productive_terms: &mut Vec<Type<'db>>,
    ) -> Option<()> {
        if let Type::Union(union) = term {
            for element in union.elements(db) {
                Self::collect_projection_component_terms(
                    db,
                    root,
                    op,
                    *element,
                    saw_self_reference,
                    productive_terms,
                )?;
            }
            return Some(());
        }

        if term.mentions_nonmatching_cycle_projection(db, root, op) {
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
        ops: &mut Vec<CycleProjectionOp>,
    ) {
        let ops = RefCell::new(ops);
        any_over_type(db, ty, false, |nested| {
            if let Type::CycleProjection(projection) = nested
                && projection.root().same_marker(root)
            {
                let mut ops = ops.borrow_mut();
                if !ops.contains(&projection.op()) {
                    ops.push(projection.op());
                }
            }
            false
        });
    }

    fn solved_projection_type(
        solved_ops: &[(CycleProjectionOp, Type<'db>)],
        op: CycleProjectionOp,
    ) -> Option<Self> {
        solved_ops
            .iter()
            .find_map(|(candidate, ty)| (*candidate == op).then_some(*ty))
    }

    fn union_solved_projection_types(
        db: &'db dyn Db,
        solved_ops: &[(CycleProjectionOp, Type<'db>)],
    ) -> Option<Self> {
        let types = solved_ops.iter().map(|(_, ty)| *ty).collect::<Vec<_>>();
        Some(match types.as_slice() {
            [] => return None,
            [ty] => *ty,
            _ => UnionType::from_elements_cycle_recovery(db, types),
        })
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
        op: CycleProjectionOp,
    ) -> bool {
        any_over_type(db, self, false, |ty| match ty {
            Type::CycleProjection(projection) => {
                projection.root().same_marker(root) && projection.op() != op
            }
            _ => false,
        })
    }
}

#[derive(Debug, Clone)]
enum ProjectionContainer<'db> {
    FixedTuple { elements: Vec<Type<'db>> },
    List { element: Type<'db> },
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

        if let Some(specialization) = ty.known_specialization(db, KnownClass::List) {
            let [element] = specialization.types(db) else {
                return None;
            };
            return Some(Self::List { element: *element });
        }

        None
    }

    fn collect_projection_ops(
        &self,
        db: &'db dyn Db,
        root: DivergentType,
        ops: &mut Vec<CycleProjectionOp>,
    ) {
        match self {
            Self::FixedTuple { elements } => {
                for element in elements {
                    Type::collect_projection_ops(db, root, *element, ops);
                }
            }
            Self::List { element } => Type::collect_projection_ops(db, root, *element, ops),
        }
    }

    fn collect_projection_terms(
        &self,
        terms_by_op: &mut [(CycleProjectionOp, Vec<ProjectionTerm<'db>>)],
    ) {
        match self {
            Self::FixedTuple { elements } => {
                let len = elements.len();
                for (index, element) in elements.iter().copied().enumerate() {
                    let op = CycleProjectionOp::UnpackExact { len, index };
                    if let Some((_, terms)) = terms_by_op
                        .iter_mut()
                        .find(|(candidate, _)| *candidate == op)
                    {
                        terms.push(ProjectionTerm::Exact(element));
                    }
                }
            }
            Self::List { element } => {
                for (_, terms) in terms_by_op.iter_mut() {
                    terms.push(ProjectionTerm::Homogeneous(*element));
                }
            }
        }
    }

    fn into_type(
        self,
        db: &'db dyn Db,
        root: DivergentType,
        solved_ops: &[(CycleProjectionOp, Type<'db>)],
    ) -> Option<Type<'db>> {
        match self {
            Self::FixedTuple { elements } => {
                let len = elements.len();
                let elements = elements
                    .into_iter()
                    .enumerate()
                    .map(|(index, element)| {
                        let op = CycleProjectionOp::UnpackExact { len, index };
                        if let Some(ty) = Type::solved_projection_type(solved_ops, op) {
                            Some(ty)
                        } else if element.mentions_cycle_artifact(db, root) {
                            None
                        } else {
                            Some(element)
                        }
                    })
                    .collect::<Option<Vec<_>>>()?;

                Some(Type::heterogeneous_tuple(db, elements))
            }
            Self::List { element } => {
                let element = if element.mentions_cycle_artifact(db, root) {
                    Type::union_solved_projection_types(db, solved_ops)?
                } else {
                    element
                };
                Some(KnownClass::List.to_specialized_instance(db, &[element]))
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ProjectionTerm<'db> {
    Exact(Type<'db>),
    Homogeneous(Type<'db>),
}

/// A query-free projection of a cycle root produced while recovering recursive inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub struct CycleProjectionType {
    root: DivergentType,
    op: CycleProjectionOp,
}

impl CycleProjectionType {
    pub(crate) const fn root(self) -> DivergentType {
        self.root
    }

    pub(crate) const fn op(self) -> CycleProjectionOp {
        self.op
    }
}

/// The projection operations currently preserved through cycle recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) enum CycleProjectionOp {
    UnpackExact { len: usize, index: usize },
}
