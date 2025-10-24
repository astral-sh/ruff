use crate::Db;
use crate::semantic_index::definition::Definition;
use crate::types::{ClassType, NormalizedVisitor};
use ruff_python_ast as ast;

/// A `typing.NewType` declaration, either from the perspective of the
/// identity-function-that-acts-like-a-subclass-in-type-expressions returned by the call to
/// `typing.NewType`, or from the perspective of instances of that subclass. For example:
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

    // The base class of this NewType (e.g. `int`), which could be a (specialized) class type or
    // could be another NewType.
    pub base: NewTypeBase<'db>,
}

impl get_size2::GetSize for NewType<'_> {}

impl<'db> NewType<'db> {
    // Walk the `NewTypeBase` chain to find the underlying `ClassType`.
    pub fn base_class_type(self, db: &'db dyn Db) -> ClassType<'db> {
        match self.base(db) {
            NewTypeBase::ClassType(base_class) => base_class,
            NewTypeBase::NewType(base_newtype) => base_newtype.base_class_type(db),
        }
    }

    pub fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        let normalized_base = match self.base(db) {
            NewTypeBase::ClassType(base_class) => {
                NewTypeBase::ClassType(base_class.normalized_impl(db, visitor))
            }
            NewTypeBase::NewType(base_newtype) => {
                NewTypeBase::NewType(base_newtype.normalized_impl(db, visitor))
            }
        };
        Self::new(
            db,
            self.name(db).clone(),
            self.definition(db),
            normalized_base,
        )
    }
}

/// `typing.NewType` typically wraps a class type, but it can also wrap another newtype. This
/// recursive enum represents these two possibilities.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub enum NewTypeBase<'db> {
    ClassType(ClassType<'db>),
    NewType(NewType<'db>),
}
