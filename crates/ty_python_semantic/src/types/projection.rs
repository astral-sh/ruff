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

        self.try_fixed_tuple_projection_cycle_normalized(db, root)
            .or_else(|| self.try_list_projection_cycle_normalized(db, root))
    }

    fn try_fixed_tuple_projection_cycle_normalized(
        self,
        db: &'db dyn Db,
        root: DivergentType,
    ) -> Option<Self> {
        let mut tuple_len = None;
        let mut component_terms: Vec<Vec<Type<'db>>> = Vec::new();
        let mut saw_tuple = false;

        for element in self.top_level_projection_union_elements(db) {
            if element.same_divergent_marker(Type::Divergent(root)) {
                continue;
            }

            let spec = element.exact_tuple_instance_spec(db)?;
            let TupleSpec::Fixed(tuple) = spec.as_ref() else {
                return None;
            };

            let len = tuple.len();
            match tuple_len {
                Some(existing_len) if existing_len != len => return None,
                Some(_) => {}
                None => {
                    tuple_len = Some(len);
                    component_terms.resize_with(len, Vec::new);
                }
            }

            for (terms, element_ty) in component_terms.iter_mut().zip(tuple.iter_all_elements()) {
                terms.push(element_ty);
            }
            saw_tuple = true;
        }

        if !saw_tuple {
            return None;
        }

        let len = tuple_len?;
        let solved_elements = component_terms
            .iter()
            .enumerate()
            .map(|(index, terms)| {
                Self::solve_projection_component(
                    db,
                    root,
                    CycleProjectionOp::UnpackExact { len, index },
                    terms,
                )
            })
            .collect::<Option<Vec<_>>>()?;

        Some(Type::heterogeneous_tuple(db, solved_elements))
    }

    fn try_list_projection_cycle_normalized(
        self,
        db: &'db dyn Db,
        root: DivergentType,
    ) -> Option<Self> {
        let mut element_terms = Vec::new();
        let mut saw_list = false;

        for element in self.top_level_projection_union_elements(db) {
            if element.same_divergent_marker(Type::Divergent(root)) {
                continue;
            }

            let specialization = element.known_specialization(db, KnownClass::List)?;
            let [element_ty] = specialization.types(db) else {
                return None;
            };

            element_terms.push(*element_ty);
            saw_list = true;
        }

        if !saw_list {
            return None;
        }

        let element_ty = Self::solve_homogeneous_projection_component(db, root, &element_terms)?;
        Some(KnownClass::List.to_specialized_instance(db, &[element_ty]))
    }

    fn top_level_projection_union_elements(self, db: &'db dyn Db) -> Vec<Self> {
        match self {
            Type::Union(union) => union.elements(db).to_vec(),
            _ => vec![self],
        }
    }

    fn solve_homogeneous_projection_component(
        db: &'db dyn Db,
        root: DivergentType,
        terms: &[Type<'db>],
    ) -> Option<Self> {
        let mut saw_self_reference = false;
        let mut productive_terms = Vec::new();

        for term in terms {
            Self::collect_homogeneous_projection_component_terms(
                db,
                root,
                *term,
                &mut saw_self_reference,
                &mut productive_terms,
            );
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

    fn solve_projection_component(
        db: &'db dyn Db,
        root: DivergentType,
        op: CycleProjectionOp,
        terms: &[Type<'db>],
    ) -> Option<Self> {
        let mut saw_self_reference = false;
        let mut productive_terms = Vec::new();

        for term in terms {
            Self::collect_projection_component_terms(
                db,
                root,
                op,
                *term,
                &mut saw_self_reference,
                &mut productive_terms,
            )?;
        }

        if productive_terms.is_empty() {
            return (!saw_self_reference).then_some(Type::Never);
        }

        Some(match productive_terms.as_slice() {
            [term] => *term,
            _ => UnionType::from_elements_cycle_recovery(db, productive_terms),
        })
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
