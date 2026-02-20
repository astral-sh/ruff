use std::collections::BTreeSet;

use crate::Db;
use crate::semantic_index::definition::{Definition, DefinitionKind};
use crate::types::constraints::ConstraintSet;
use crate::types::{ClassType, KnownUnion, Type, definition_expression_type, visitor};
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;

/// A `typing.NewType` declaration, either from the perspective of the
/// identity-callable-that-acts-like-a-subtype-in-type-expressions returned by the call to
/// `typing.NewType(...)`, or from the perspective of instances of that subtype returned by the
/// identity callable. For example:
///
/// ```py
/// import typing
/// Foo = typing.NewType("Foo", int)
/// x = Foo(42)
/// ```
///
/// The revealed types there are:
/// - `typing.NewType`: `Type::ClassLiteral(ClassLiteral)` with `KnownClass::NewType`.
/// - `Foo`: `Type::KnownInstance(KnownInstanceType::NewType(NewType { .. }))`
/// - `x`: `Type::NewTypeInstance(NewType { .. })`
///
/// # Ordering
/// Ordering is based on the newtype's salsa-assigned id and not on its values.
/// The id may change between runs, or when the newtype was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct NewType<'db> {
    /// The name of this NewType (e.g. `"Foo"`)
    #[returns(ref)]
    pub name: ast::name::Name,

    /// The binding where this NewType is first created.
    pub definition: Definition<'db>,

    // The base type of this NewType, if it's eagerly specified. This is typically `None` when a
    // `NewType` is first encountered, because the base type is lazy/deferred to avoid panics in
    // the recursive case. This becomes `Some` when a `NewType` is modified by methods like
    // `.normalize()`. Callers should use the `base` method instead of accessing this field
    // directly.
    eager_base: Option<NewTypeBase<'db>>,
}

impl get_size2::GetSize for NewType<'_> {}

#[salsa::tracked]
impl<'db> NewType<'db> {
    pub fn base(self, db: &'db dyn Db) -> NewTypeBase<'db> {
        match self.eager_base(db) {
            Some(base) => base,
            None => self.lazy_base(db),
        }
    }

    #[salsa::tracked(
        cycle_initial=lazy_base_cycle_initial,
        heap_size=ruff_memory_usage::heap_size
    )]
    fn lazy_base(self, db: &'db dyn Db) -> NewTypeBase<'db> {
        // `TypeInferenceBuilder` emits diagnostics for invalid `NewType` definitions that show up
        // in assignments, but invalid definitions still get here, and also `NewType` might show up
        // in places that aren't definitions at all. Fall back to `object` in all error cases.
        let object_fallback = NewTypeBase::ClassType(ClassType::object(db));
        let definition = self.definition(db);
        let module = parsed_module(db, definition.file(db)).load(db);
        let DefinitionKind::Assignment(assignment) = definition.kind(db) else {
            return object_fallback;
        };
        let Some(call_expr) = assignment.value(&module).as_call_expr() else {
            return object_fallback;
        };
        let Some(second_arg) = call_expr.arguments.args.get(1) else {
            return object_fallback;
        };
        match definition_expression_type(db, definition, second_arg) {
            Type::NominalInstance(nominal_instance_type) => {
                NewTypeBase::ClassType(nominal_instance_type.class(db))
            }
            Type::NewTypeInstance(newtype) => NewTypeBase::NewType(newtype),
            // There are exactly two union types allowed as bases for NewType: `int | float` and
            // `int | float | complex`. These are allowed because that's what `float` and `complex`
            // expand into in type position. We don't currently ask whether the union was implicit
            // or explicit, so the explicit version is also allowed.
            Type::Union(union_type) => match union_type.known(db) {
                Some(KnownUnion::Float) => NewTypeBase::Float,
                Some(KnownUnion::Complex) => NewTypeBase::Complex,
                _ => object_fallback,
            },
            _ => object_fallback,
        }
    }

    fn iter_bases(self, db: &'db dyn Db) -> NewTypeBaseIter<'db> {
        NewTypeBaseIter {
            current: Some(self),
            seen_before: BTreeSet::new(),
            db,
        }
    }

    // Walk the `NewTypeBase` chain to find the underlying non-newtype `Type`. There might not be
    // one if this `NewType` is cyclical, and we fall back to `object` in that case.
    pub fn concrete_base_type(self, db: &'db dyn Db) -> Type<'db> {
        for base in self.iter_bases(db) {
            match base {
                NewTypeBase::NewType(_) => continue,
                concrete => return concrete.instance_type(db),
            }
        }
        Type::object()
    }

    pub(crate) fn is_equivalent_to_impl(self, db: &'db dyn Db, other: Self) -> bool {
        // Two instances of the "same" `NewType` won't compare == if one of them has an eagerly
        // evaluated base (or a normalized base, etc.) and the other doesn't, so we only check for
        // equality of the `definition`.
        self.definition(db) == other.definition(db)
    }

    // Since a regular class can't inherit from a newtype, the only way for one newtype to be a
    // subtype of another is to have the other in its chain of newtype bases. Once we reach the
    // base class, we don't have to keep looking.
    pub(crate) fn has_relation_to_impl(self, db: &'db dyn Db, other: Self) -> ConstraintSet<'db> {
        if self.is_equivalent_to_impl(db, other) {
            return ConstraintSet::from(true);
        }
        for base in self.iter_bases(db) {
            if let NewTypeBase::NewType(base_newtype) = base {
                if base_newtype.is_equivalent_to_impl(db, other) {
                    return ConstraintSet::from(true);
                }
            }
        }
        ConstraintSet::from(false)
    }

    pub(crate) fn is_disjoint_from_impl(self, db: &'db dyn Db, other: Self) -> ConstraintSet<'db> {
        // Two NewTypes are disjoint if they're not equal and neither inherits from the other.
        // NewTypes have single inheritance, and a regular class can't inherit from a NewType, so
        // it's not possible for some third type to multiply-inherit from both.
        let mut self_not_subtype_of_other = self.has_relation_to_impl(db, other).negate(db);
        let other_not_subtype_of_self = other.has_relation_to_impl(db, self).negate(db);
        self_not_subtype_of_other.intersect(db, other_not_subtype_of_self)
    }

    /// Create a new `NewType` by mapping the underlying `ClassType`. This descends through any
    /// number of nested `NewType` layers and rebuilds the whole chain. In the rare case of cyclic
    /// `NewType`s with no underlying `ClassType`, this has no effect and does not call `f`.
    pub(crate) fn try_map_base_class_type(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(ClassType<'db>) -> Option<ClassType<'db>>,
    ) -> Option<Self> {
        // Modifying the base class type requires unwrapping and re-wrapping however many base
        // newtypes there are between here and there. Normally recursion would be natural for this,
        // but the bases iterator does cycle detection, and I think using that with a stack is a
        // little cleaner than conjuring up yet another `CycleDetector` visitor and yet another
        // layer of "*_impl" nesting. Also if there is no base class type, returning `self`
        // unmodified seems more correct than injecting some default type like `object` into the
        // cycle, which is what `CycleDetector` would do if we used it here.
        let mut inner_newtype_stack = Vec::new();
        for base in self.iter_bases(db) {
            match base {
                // Build up the stack of intermediate newtypes that we'll need to re-wrap after
                // we've mapped the `ClassType`.
                NewTypeBase::NewType(base_newtype) => inner_newtype_stack.push(base_newtype),
                // We've reached the `ClassType`.
                NewTypeBase::ClassType(base_class_type) => {
                    // Call `f`.
                    let mut mapped_base = NewTypeBase::ClassType(f(base_class_type)?);
                    // Re-wrap the mapped base class in however many newtypes we unwrapped.
                    for inner_newtype in inner_newtype_stack.into_iter().rev() {
                        mapped_base = NewTypeBase::NewType(NewType::new(
                            db,
                            inner_newtype.name(db).clone(),
                            inner_newtype.definition(db),
                            Some(mapped_base),
                        ));
                    }
                    return Some(NewType::new(
                        db,
                        self.name(db).clone(),
                        self.definition(db),
                        Some(mapped_base),
                    ));
                }
                // Mapping base class types is used for normalization and applying type mappings,
                // neither of which have any effect on `float` or `complex` (which are already
                // fully normalized and non-generic), so we don't need to bother calling `f`.
                NewTypeBase::Float | NewTypeBase::Complex => {}
            }
        }
        // If we get here, there is no `ClassType` (because this newtype is either float/complex or
        // cyclic), and we don't call `f` at all.
        Some(self)
    }

    pub(crate) fn map_base_class_type(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(ClassType<'db>) -> ClassType<'db>,
    ) -> Self {
        self.try_map_base_class_type(db, |class_type| Some(f(class_type)))
            .unwrap()
    }
}

pub(crate) fn walk_newtype_instance_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    newtype: NewType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, newtype.base(db).instance_type(db));
}

/// `typing.NewType` typically wraps a class type, but it can also wrap another newtype.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub enum NewTypeBase<'db> {
    ClassType(ClassType<'db>),
    NewType(NewType<'db>),
    // `float` and `complex` are special-cased in type position, where they refer to `int | float`
    // and `int | float | complex` respectively. As an extension of that special case, we allow
    // them in `NewType` bases, even though unions and other typing constructs normally aren't
    // allowed.
    Float,
    Complex,
}

impl<'db> NewTypeBase<'db> {
    pub fn instance_type(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            NewTypeBase::ClassType(class_type) => Type::instance(db, class_type),
            NewTypeBase::NewType(newtype) => Type::NewTypeInstance(newtype),
            NewTypeBase::Float => KnownUnion::Float.to_type(db),
            NewTypeBase::Complex => KnownUnion::Complex.to_type(db),
        }
    }
}

/// An iterator over the transitive bases of a `NewType`. In the most common case, e.g.
/// `Foo = NewType("Foo", int)`, this yields the one `NewTypeBase::ClassType` (e.g. `int`). For
/// newtypes that wrap other newtypes, this iterator yields the `NewTypeBase::NewType`s (not
/// including `self`) before finally yielding the `NewTypeBase::ClassType`. In the pathological
/// case of cyclic newtypes like `Foo = NewType("Foo", "Foo")`, this iterator yields the unique
/// `NewTypeBase::NewType`s (not including `self`), detects the cycle, and then stops.
///
/// Note that this does *not* detect indirect cycles that go through a proper class, like this:
/// ```py
/// Foo = NewType("Foo", list["Foo"])
/// ```
/// As far as this iterator is concerned, that's the "common case", and it yields the one
/// `NewTypeBase::ClassType` for `list[Foo]`. Functions like `normalize` that continue recursing
/// over the base class need to pass down a cycle-detecting visitor as usual.
struct NewTypeBaseIter<'db> {
    current: Option<NewType<'db>>,
    seen_before: BTreeSet<NewType<'db>>,
    db: &'db dyn Db,
}

impl<'db> Iterator for NewTypeBaseIter<'db> {
    type Item = NewTypeBase<'db>;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current?;
        match current.base(self.db) {
            NewTypeBase::NewType(base_newtype) => {
                // Doing the insertion only in this branch avoids allocating in the common case.
                self.seen_before.insert(current);
                if self.seen_before.contains(&base_newtype) {
                    // Cycle detected. Stop iterating.
                    self.current = None;
                    None
                } else {
                    self.current = Some(base_newtype);
                    Some(NewTypeBase::NewType(base_newtype))
                }
            }
            concrete_base => {
                self.current = None;
                Some(concrete_base)
            }
        }
    }
}

fn lazy_base_cycle_initial<'db>(
    db: &'db dyn Db,
    _id: salsa::Id,
    _self: NewType<'db>,
) -> NewTypeBase<'db> {
    NewTypeBase::ClassType(ClassType::object(db))
}
