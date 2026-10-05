#![cfg(feature = "inventory")]

//! Test for cycle handling where a tracked struct created in the first revision
//! is stored in the final value of the cycle but isn't recreated in the second
//! iteration of the creating query.
//!
//! It's important that the creating query in the last iteration keeps *owning* the
//! tracked struct from the previous iteration, otherwise Salsa will discard it
//! and dereferencing the value panics.
mod common;

use crate::common::{EventLoggerDatabase, LogDatabase};
use expect_test::expect;
use salsa::{Database, Setter};

#[salsa::input(debug)]
struct ClassNode {
    name: String,
    #[returns(copy)]
    type_params: Option<TypeParamNode>,
}

#[salsa::input(debug)]
struct TypeParamNode {
    name: String,
    #[returns(copy)]
    constraint: Option<ClassNode>,
}

#[salsa::interned(debug)]
struct Class<'db> {
    name: String,
    #[returns(copy)]
    type_params: Option<TypeParam<'db>>,
}

#[salsa::tracked(debug)]
struct TypeParam<'db> {
    name: String,
    #[returns(copy)]
    constraint: Option<Type<'db>>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, salsa::SalsaValue)]
enum Type<'db> {
    Class(Class<'db>),
    Unknown,
}

impl Type<'_> {
    fn class(&self) -> Option<Class<'_>> {
        match self {
            Type::Class(class) => Some(*class),
            Type::Unknown => None,
        }
    }
}

#[salsa::tracked(returns(copy), cycle_initial=infer_class_initial)]
fn infer_class(db: &dyn salsa::Database, node: ClassNode) -> Type<'_> {
    Type::Class(Class::new(
        db,
        node.name(db),
        node.type_params(db).map(|tp| infer_type_param(db, tp)),
    ))
}

#[salsa::tracked(returns(copy))]
fn infer_type_param(db: &dyn salsa::Database, node: TypeParamNode) -> TypeParam<'_> {
    if let Some(constraint) = node.constraint(db) {
        // Reuse the type param from the class if any.
        // The example is a bit silly, because it's a reduction of what we have in Astral's type checker
        // but including all the details doesn't make sense. What's important for the test is
        // that this query doesn't re-create the `TypeParam` tracked struct in the second iteration
        // and instead returns the one from the first iteration which
        // then is returned in the overall result (Class).
        match infer_class(db, constraint) {
            Type::Class(class) => class
                .type_params(db)
                .unwrap_or_else(|| TypeParam::new(db, node.name(db).clone(), Some(Type::Unknown))),
            Type::Unknown => TypeParam::new(db, node.name(db).clone(), Some(Type::Unknown)),
        }
    } else {
        TypeParam::new(db, node.name(db).clone(), None)
    }
}

fn infer_class_initial(_db: &'_ dyn Database, _id: salsa::Id, _node: ClassNode) -> Type<'_> {
    Type::Unknown
}

#[test]
fn main() {
    let mut db = EventLoggerDatabase::default();

    // Class with a type parameter that's constrained to itself.
    // class Test[T: Test]: ...
    let class_node = ClassNode::new(&db, "Test".to_string(), None);
    let type_param_node = TypeParamNode::new(&db, "T".to_string(), Some(class_node));
    class_node
        .set_type_params(&mut db)
        .to(Some(type_param_node));

    let ty = infer_class(&db, class_node);

    db.assert_logs(expect![[r#"
        [
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "WillExecute { database_key: infer_class(Id(0)) }",
            "WillCheckCancellation",
            "WillExecute { database_key: infer_type_param(Id(80)) }",
            "WillCheckCancellation",
            "DidInternValue { key: Class(Id(180)), revision: R2 }",
            "WillIterateCycle { database_key: infer_class(Id(0)), iteration: 1 }",
            "WillCheckCancellation",
            "WillExecute { database_key: infer_type_param(Id(80)) }",
            "WillCheckCancellation",
            "DidFinalizeCycle { database_key: infer_class(Id(0)), iteration: 1 }",
        ]"#]]);

    let class = ty.class().unwrap();
    let type_param = class.type_params(&db).unwrap();

    // Now read the name from the type param struct that was created in the first iteration of
    // `infer_type_param`. This should not panic!
    assert_eq!(type_param.name(&db), "T");
}
