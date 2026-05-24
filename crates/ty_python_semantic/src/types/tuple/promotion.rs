use rustc_hash::FxHashSet;

use ruff_python_ast as ast;

use crate::Db;
use crate::types::tuple::TupleSpec;
use crate::types::typevar::BoundTypeVarIdentity;
use crate::types::visitor::any_over_type;
use crate::types::{Type, UnionBuilder};

/// Tracks the typevars of a collection to which tuple size promotion should **not** apply.
#[derive(Default)]
pub(crate) struct TupleSizePromotionConstraints<'db> {
    blocked_typevars: FxHashSet<BoundTypeVarIdentity<'db>>,
}

impl<'db> TupleSizePromotionConstraints<'db> {
    /// Records that a typevar has a declared type. This makes it ineligible for tuple size promotion.
    pub(crate) fn record_declared_type(&mut self, typevar_identity: BoundTypeVarIdentity<'db>) {
        self.blocked_typevars.insert(typevar_identity);
    }

    /// Records whether an inferred collection element blocks tuple size promotion for the typevar.
    pub(crate) fn record_inferred_expression_type(
        &mut self,
        db: &'db dyn Db,
        typevar_identity: BoundTypeVarIdentity<'db>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) {
        if !Self::is_promotable_tuple_literal(db, expression, ty) {
            self.record_unpromotable_type(db, typevar_identity, ty);
        }
    }

    /// Records that a typevar is ineligible for tuple size promotion if the given type contains
    /// a tuple type.
    pub(crate) fn record_unpromotable_type(
        &mut self,
        db: &'db dyn Db,
        typevar_identity: BoundTypeVarIdentity<'db>,
        ty: Type<'db>,
    ) {
        if any_over_type(db, ty, true, |ty| ty.tuple_instance_spec(db).is_some()) {
            self.blocked_typevars.insert(typevar_identity);
        }
    }

    /// Reports whether or not tuple size promotion is allowed for the given typevar in light
    /// of the constraints recorded on this object.
    pub(crate) fn allow(&self, typevar_identity: BoundTypeVarIdentity<'db>) -> bool {
        !self.blocked_typevars.contains(&typevar_identity)
    }

    /// Returns true if the given expression is either a non-starred homogeneous tuple literal or the
    /// empty tuple (and hence is eligible for tuple size promotion).
    fn is_promotable_tuple_literal(db: &'db dyn Db, expression: &ast::Expr, ty: Type<'db>) -> bool {
        matches!(expression, ast::Expr::Tuple(tuple) if !tuple.iter().any(ast::Expr::is_starred_expr))
            && TupleSizePromotionCandidate::from_type(db, ty).is_some()
    }
}

/// Represents a single tuple literal whose type in the inferred collection type might be widened.
enum TupleSizePromotionCandidate<'db> {
    Empty,
    Homogeneous {
        element_type: Type<'db>,
        length: usize,
    },
}

impl<'db> TupleSizePromotionCandidate<'db> {
    /// Returns an eligible candidate if the given type represents one (i.e., it is a
    /// fixed-length homogeneous tuple or the empty tuple).
    fn from_type(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        let tuple_spec = ty.exact_tuple_instance_spec(db)?;
        let TupleSpec::Fixed(tuple) = tuple_spec.as_ref() else {
            return None;
        };

        let mut elements = tuple.iter_all_elements();
        let Some(element_type) = elements.next() else {
            return Some(Self::Empty);
        };

        elements
            .all(|element| element.is_equivalent_to(db, element_type))
            .then_some(Self::Homogeneous {
                element_type,
                length: tuple.len(),
            })
    }
}

/// Represents a group of tuple types extracted from a larger union. The types in this group may
/// be widened in the final inferred type for the collection literal.
struct HomogeneousTupleUnionGroup<'db> {
    element_type: Type<'db>,
    original_tuple_types: Vec<Type<'db>>,
    first_length: usize,
    has_multiple_lengths: bool,
}

impl<'db> HomogeneousTupleUnionGroup<'db> {
    fn new(element_type: Type<'db>, original_tuple_type: Type<'db>, length: usize) -> Self {
        Self {
            element_type,
            original_tuple_types: vec![original_tuple_type],
            first_length: length,
            has_multiple_lengths: false,
        }
    }

    /// Adds a tuple to this homogeneous union group.
    fn add(&mut self, original_tuple_type: Type<'db>, length: usize) {
        self.has_multiple_lengths |= length != self.first_length;
        self.original_tuple_types.push(original_tuple_type);
    }
}

/// Partitions a union into two sets prior to rebuilding it: one for elements that are not
/// candidates for tuple size promotion, and another for groups of homogeneous tuple elements that are.
fn partition_tuple_union_elements<'db>(
    db: &'db dyn Db,
    elements: impl IntoIterator<Item = Type<'db>>,
) -> (Vec<Type<'db>>, Vec<HomogeneousTupleUnionGroup<'db>>) {
    let mut other_union_elements = Vec::new();
    let mut tuple_groups: Vec<HomogeneousTupleUnionGroup<'db>> = Vec::new();

    for element in elements {
        match TupleSizePromotionCandidate::from_type(db, element) {
            Some(TupleSizePromotionCandidate::Homogeneous {
                element_type,
                length,
            }) => {
                if let Some(group) = tuple_groups
                    .iter_mut()
                    .find(|group| group.element_type.is_equivalent_to(db, element_type))
                {
                    group.add(element, length);
                } else {
                    tuple_groups.push(HomogeneousTupleUnionGroup::new(
                        element_type,
                        element,
                        length,
                    ));
                }
            }
            Some(TupleSizePromotionCandidate::Empty) | None => other_union_elements.push(element),
        }
    }

    (other_union_elements, tuple_groups)
}

impl<'db> Type<'db> {
    /// Within a larger union, promotes every group of homogeneous, fixed-length tuples of differing
    /// lengths to a single variadic tuple.
    ///
    /// This deliberately only applies to unions; a standalone tuple keeps its shape.
    ///
    /// The caller is responsible for checking that every tuple source that contributes to this
    /// union is eligible for promotion (see [`TupleSizePromotionConstraints`]).
    ///
    /// # Example
    ///
    /// In the code below, we promote `dict[str, tuple[str, str] | tuple[str, str, str, str]]`
    /// to `dict[str, tuple[str, ...]]`:
    ///
    /// ```python
    /// languages = {
    ///     "python": (".py", ".pyi"),
    ///     "javascript": (".js", ".jsx", ".ts", ".tsx"),
    /// }
    /// reveal_type(languages)  # revealed: dict[str, tuple[str, ...]]
    /// ```
    ///
    pub(crate) fn promote_tuple_size_in_union(self, db: &'db dyn Db) -> Type<'db> {
        let Type::Union(union) = self else {
            return self;
        };

        let (other_union_elements, tuple_groups) =
            partition_tuple_union_elements(db, union.elements(db).iter().copied());

        if !tuple_groups.iter().any(|group| group.has_multiple_lengths) {
            return self;
        }

        let mut builder = UnionBuilder::new(db)
            .unpack_aliases(false)
            .recursively_defined(union.recursively_defined(db));

        for element in other_union_elements {
            builder = builder.add(element);
        }

        for group in tuple_groups {
            if group.has_multiple_lengths {
                builder = builder.add(Type::homogeneous_tuple(db, group.element_type));
            } else {
                for element in group.original_tuple_types {
                    builder = builder.add(element);
                }
            }
        }

        builder.build()
    }
}
