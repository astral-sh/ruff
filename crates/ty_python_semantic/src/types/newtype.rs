use std::collections::BTreeSet;

use crate::Db;
use crate::semantic_index::definition::{Definition, DefinitionKind};
use crate::types::constraints::ConstraintSet;
use crate::types::{ClassType, Type, definition_expression_type};
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;

/// A `typing.NewType` declaration, either from the perspective of the
/// identity-function-that-acts-like-a-subtype-in-type-expressions returned by the call to
/// `typing.NewType`, or from the perspective of instances of that subtype returned by the
/// identity function. For example:
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

    #[salsa::tracked]
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
            // This branch includes bases that are other typing constructs besides classes and
            // other newtypes, for example unions. `NewType("Foo", int | str)` is not allowed.
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

    // Walk the `NewTypeBase` chain to find the underlying `ClassType`. There might not be a
    // `ClassType` if this `NewType` is cyclical, and we fall back to `object` in that case.
    pub fn base_class_type(self, db: &'db dyn Db) -> ClassType<'db> {
        for base in self.iter_bases(db) {
            if let NewTypeBase::ClassType(class_type) = base {
                return class_type;
            }
        }
        ClassType::object(db)
    }

    // Since a regular class can't inherit from a newtype, the only way for one newtype to be a
    // subtype of another is to have the other in its chain of newtype bases. Once we reach the
    // base class, we don't have to keep looking.
    pub(crate) fn is_subtype_of(self, db: &'db dyn Db, other: Self) -> bool {
        // Two instances of the "same" `NewType` won't compare equal if one of them has an eagerly
        // evaluated base (or a normalized base, etc.) and the other doesn't, so we only check for
        // equality of the `definition`.
        if self.definition(db) == other.definition(db) {
            return true;
        }
        for base in self.iter_bases(db) {
            if let NewTypeBase::NewType(base_newtype) = base {
                if base_newtype.definition(db) == other.definition(db) {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn has_relation_to_impl(self, db: &'db dyn Db, other: Self) -> ConstraintSet<'db> {
        // REVIEWERS: Is this correct?
        ConstraintSet::from(self.is_subtype_of(db, other))
    }

    pub(crate) fn is_disjoint_from_impl(self, db: &'db dyn Db, other: Self) -> ConstraintSet<'db> {
        // Two NewTypes are disjoint if they're not equal and neither inherits from the other.
        // NewTypes have single inheritance, and a regular class can't inherit from a NewType, so
        // it's not possible for some third type to multiply-inherit from both.
        ConstraintSet::from(!self.is_subtype_of(db, other) && !other.is_subtype_of(db, self))
    }

    /// Create a new `NewType` by mapping the underlying `ClassType`. This descends through any
    /// number of nested `NewType` layers and rebuilds the whole chain. In the rare case of cyclic
    /// `NewType`s with no underlying `ClassType`, this has no effect and does not call `f`.
    pub(crate) fn map_base_class_type(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(ClassType<'db>) -> ClassType<'db>,
    ) -> Self {
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
                    let mut mapped_base = NewTypeBase::ClassType(f(base_class_type));
                    // Re-wrap the mapped base class in however many newtypes we unwrapped.
                    for inner_newtype in inner_newtype_stack.into_iter().rev() {
                        mapped_base = NewTypeBase::NewType(NewType::new(
                            db,
                            inner_newtype.name(db).clone(),
                            inner_newtype.definition(db),
                            Some(mapped_base),
                        ));
                    }
                    return NewType::new(
                        db,
                        self.name(db).clone(),
                        self.definition(db),
                        Some(mapped_base),
                    );
                }
            }
        }
        // If we get here, there is no `ClassType` (because this newtype is cyclic), and we don't
        // call `f` at all.
        self
    }
}

/// `typing.NewType` typically wraps a class type, but it can also wrap another newtype.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub enum NewTypeBase<'db> {
    ClassType(ClassType<'db>),
    NewType(NewType<'db>),
}

impl<'db> NewTypeBase<'db> {
    pub fn instance_type(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            NewTypeBase::ClassType(class_type) => Type::instance(db, class_type),
            NewTypeBase::NewType(newtype) => Type::NewTypeInstance(newtype),
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
            NewTypeBase::ClassType(base_class_type) => {
                self.current = None;
                Some(NewTypeBase::ClassType(base_class_type))
            }
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
        }
    }
}
