use crate::db::tests::TestDb;
use crate::symbol::{builtins_symbol, known_module_symbol};
use crate::types::{
    BoundMethodType, CallableType, IntersectionBuilder, KnownClass, KnownInstanceType,
    SubclassOfType, TupleType, Type, UnionType,
};
use crate::{Db, KnownModule};

/// A test representation of a type that can be transformed unambiguously into a real Type,
/// given a db.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Ty {
    Never,
    Unknown,
    None,
    Any,
    IntLiteral(i64),
    BooleanLiteral(bool),
    StringLiteral(&'static str),
    LiteralString,
    BytesLiteral(&'static str),
    // BuiltinInstance("str") corresponds to an instance of the builtin `str` class
    BuiltinInstance(&'static str),
    /// Members of the `abc` stdlib module
    AbcInstance(&'static str),
    AbcClassLiteral(&'static str),
    TypingLiteral,
    // BuiltinClassLiteral("str") corresponds to the builtin `str` class object itself
    BuiltinClassLiteral(&'static str),
    KnownClassInstance(KnownClass),
    Union(Vec<Ty>),
    Intersection {
        pos: Vec<Ty>,
        neg: Vec<Ty>,
    },
    Tuple(Vec<Ty>),
    SubclassOfAny,
    SubclassOfBuiltinClass(&'static str),
    SubclassOfAbcClass(&'static str),
    AlwaysTruthy,
    AlwaysFalsy,
    BuiltinsFunction(&'static str),
    BuiltinsBoundMethod {
        class: &'static str,
        method: &'static str,
    },
}

#[salsa::tracked]
fn create_bound_method<'db>(
    db: &'db dyn Db,
    function: Type<'db>,
    builtins_class: Type<'db>,
) -> Type<'db> {
    Type::Callable(CallableType::BoundMethod(BoundMethodType::new(
        db,
        function.expect_function_literal(),
        builtins_class.to_instance(db).unwrap(),
    )))
}

impl Ty {
    pub(crate) fn into_type(self, db: &TestDb) -> Type<'_> {
        match self {
            Ty::Never => Type::Never,
            Ty::Unknown => Type::unknown(),
            Ty::None => Type::none(db),
            Ty::Any => Type::any(),
            Ty::IntLiteral(n) => Type::IntLiteral(n),
            Ty::StringLiteral(s) => Type::string_literal(db, s),
            Ty::BooleanLiteral(b) => Type::BooleanLiteral(b),
            Ty::LiteralString => Type::LiteralString,
            Ty::BytesLiteral(s) => Type::bytes_literal(db, s.as_bytes()),
            Ty::BuiltinInstance(s) => builtins_symbol(db, s)
                .symbol
                .expect_type()
                .to_instance(db)
                .unwrap(),
            Ty::AbcInstance(s) => known_module_symbol(db, KnownModule::Abc, s)
                .symbol
                .expect_type()
                .to_instance(db)
                .unwrap(),
            Ty::AbcClassLiteral(s) => known_module_symbol(db, KnownModule::Abc, s)
                .symbol
                .expect_type(),
            Ty::TypingLiteral => Type::KnownInstance(KnownInstanceType::Literal),
            Ty::BuiltinClassLiteral(s) => builtins_symbol(db, s).symbol.expect_type(),
            Ty::KnownClassInstance(known_class) => known_class.to_instance(db),
            Ty::Union(tys) => {
                UnionType::from_elements(db, tys.into_iter().map(|ty| ty.into_type(db)))
            }
            Ty::Intersection { pos, neg } => {
                let mut builder = IntersectionBuilder::new(db);
                for p in pos {
                    builder = builder.add_positive(p.into_type(db));
                }
                for n in neg {
                    builder = builder.add_negative(n.into_type(db));
                }
                builder.build()
            }
            Ty::Tuple(tys) => {
                let elements = tys.into_iter().map(|ty| ty.into_type(db));
                TupleType::from_elements(db, elements)
            }
            Ty::SubclassOfAny => SubclassOfType::subclass_of_any(),
            Ty::SubclassOfBuiltinClass(s) => SubclassOfType::from(
                db,
                builtins_symbol(db, s)
                    .symbol
                    .expect_type()
                    .expect_class_literal()
                    .class,
            ),
            Ty::SubclassOfAbcClass(s) => SubclassOfType::from(
                db,
                known_module_symbol(db, KnownModule::Abc, s)
                    .symbol
                    .expect_type()
                    .expect_class_literal()
                    .class,
            ),
            Ty::AlwaysTruthy => Type::AlwaysTruthy,
            Ty::AlwaysFalsy => Type::AlwaysFalsy,
            Ty::BuiltinsFunction(name) => builtins_symbol(db, name).symbol.expect_type(),
            Ty::BuiltinsBoundMethod { class, method } => {
                let builtins_class = builtins_symbol(db, class).symbol.expect_type();
                let function = builtins_class.member(db, method).symbol.expect_type();

                create_bound_method(db, function, builtins_class)
            }
        }
    }
}
