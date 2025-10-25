use crate::Db;
use crate::semantic_index::definition::{Definition, DefinitionKind};
use crate::types::{ClassType, NormalizedVisitor, Type, definition_expression_type};
use ruff_db::parsed::parsed_module;
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

    // The base type of this NewType, if it's eagerly specified. This is typically `None` when a
    // `NewType` is first encountered, because the base type is lazy/deferred, which is necessary
    // to avoid panics in the recursive case. This becomes `Some` when a `NewType` is modified by
    // methods like `.normalize()`. Callers should use the `base` method instead of accessing this
    // field directly.
    eager_base: Option<NewTypeBase<'db>>,
}

impl get_size2::GetSize for NewType<'_> {}

#[salsa::tracked]
impl<'db> NewType<'db> {
    fn base(self, db: &'db dyn Db) -> NewTypeBase<'db> {
        match self.eager_base(db) {
            Some(base) => base,
            None => self.lazy_base(db),
        }
    }

    #[salsa::tracked(
        cycle_initial=lazy_base_initial,
        heap_size=ruff_memory_usage::heap_size
    )]
    fn lazy_base(self, db: &'db dyn Db) -> NewTypeBase<'db> {
        let definition = self.definition(db);
        let module = parsed_module(db, definition.file(db)).load(db);
        let DefinitionKind::Assignment(assignment) = definition.kind(db) else {
            unreachable!("TODO?!?!");
        };
        let call_expr = assignment
            .value(&module)
            .as_call_expr()
            .expect("TODO UNREACHABLE?");
        let second_arg = call_expr.arguments.args.get(1).expect("TODO UNREACHABLE?");
        match definition_expression_type(db, definition, second_arg) {
            Type::NominalInstance(nominal_instance_type) => {
                NewTypeBase::ClassType(nominal_instance_type.class(db))
            }
            x => {
                dbg!(x);
                panic!("oh no");
            }
        }
    }

    // Walk the `NewTypeBase` chain to find the underlying `ClassType`.
    pub fn base_class_type(self, db: &'db dyn Db) -> ClassType<'db> {
        match self.lazy_base(db) {
            NewTypeBase::ClassType(base_class) => base_class,
            NewTypeBase::NewType(base_newtype) => base_newtype.base_class_type(db),
        }
    }

    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        let normalized_base = match self.lazy_base(db) {
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
            Some(normalized_base),
        )
    }
}

fn lazy_base_initial<'db>(_db: &'db dyn Db, _self: NewType<'db>) -> NewTypeBase<'db> {
    todo!("what should this be? unknown?");
}

/// `typing.NewType` typically wraps a class type, but it can also wrap another newtype. This
/// recursive enum represents these two possibilities.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub enum NewTypeBase<'db> {
    ClassType(ClassType<'db>),
    NewType(NewType<'db>),
}
