use crate::db::tests::TestDb;
use crate::symbol::{builtins_symbol, known_module_symbol};
use crate::types::{
    BoundMethodType, CallableType, IntersectionBuilder, KnownClass, KnownInstanceType,
    SubclassOfType, TupleType, Type, UnionType,
};
use crate::{Db, KnownModule};
use quickcheck::{Arbitrary, Gen};

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

fn arbitrary_core_type(g: &mut Gen) -> Ty {
    // We could select a random integer here, but this would make it much less
    // likely to explore interesting edge cases:
    let int_lit = Ty::IntLiteral(*g.choose(&[-2, -1, 0, 1, 2]).unwrap());
    let bool_lit = Ty::BooleanLiteral(bool::arbitrary(g));
    g.choose(&[
        Ty::Never,
        Ty::Unknown,
        Ty::None,
        Ty::Any,
        int_lit,
        bool_lit,
        Ty::StringLiteral(""),
        Ty::StringLiteral("a"),
        Ty::LiteralString,
        Ty::BytesLiteral(""),
        Ty::BytesLiteral("\x00"),
        Ty::KnownClassInstance(KnownClass::Object),
        Ty::KnownClassInstance(KnownClass::Str),
        Ty::KnownClassInstance(KnownClass::Int),
        Ty::KnownClassInstance(KnownClass::Bool),
        Ty::KnownClassInstance(KnownClass::List),
        Ty::KnownClassInstance(KnownClass::Tuple),
        Ty::KnownClassInstance(KnownClass::FunctionType),
        Ty::KnownClassInstance(KnownClass::SpecialForm),
        Ty::KnownClassInstance(KnownClass::TypeVar),
        Ty::KnownClassInstance(KnownClass::TypeAliasType),
        Ty::KnownClassInstance(KnownClass::NoDefaultType),
        Ty::TypingLiteral,
        Ty::BuiltinClassLiteral("str"),
        Ty::BuiltinClassLiteral("int"),
        Ty::BuiltinClassLiteral("bool"),
        Ty::BuiltinClassLiteral("object"),
        Ty::BuiltinInstance("type"),
        Ty::AbcInstance("ABC"),
        Ty::AbcInstance("ABCMeta"),
        Ty::SubclassOfAny,
        Ty::SubclassOfBuiltinClass("object"),
        Ty::SubclassOfBuiltinClass("str"),
        Ty::SubclassOfBuiltinClass("type"),
        Ty::AbcClassLiteral("ABC"),
        Ty::AbcClassLiteral("ABCMeta"),
        Ty::SubclassOfAbcClass("ABC"),
        Ty::SubclassOfAbcClass("ABCMeta"),
        Ty::AlwaysTruthy,
        Ty::AlwaysFalsy,
        Ty::BuiltinsFunction("chr"),
        Ty::BuiltinsFunction("ascii"),
        Ty::BuiltinsBoundMethod {
            class: "str",
            method: "isascii",
        },
        Ty::BuiltinsBoundMethod {
            class: "int",
            method: "bit_length",
        },
    ])
    .unwrap()
    .clone()
}

/// Constructs an arbitrary type.
///
/// The `size` parameter controls the depth of the type tree. For example,
/// a simple type like `int` has a size of 0, `Union[int, str]` has a size
/// of 1, `tuple[int, Union[str, bytes]]` has a size of 2, etc.
fn arbitrary_type(g: &mut Gen, size: u32) -> Ty {
    if size == 0 {
        arbitrary_core_type(g)
    } else {
        match u32::arbitrary(g) % 4 {
            0 => arbitrary_core_type(g),
            1 => Ty::Union(
                (0..*g.choose(&[2, 3]).unwrap())
                    .map(|_| arbitrary_type(g, size - 1))
                    .collect(),
            ),
            2 => Ty::Tuple(
                (0..*g.choose(&[0, 1, 2]).unwrap())
                    .map(|_| arbitrary_type(g, size - 1))
                    .collect(),
            ),
            3 => Ty::Intersection {
                pos: (0..*g.choose(&[0, 1, 2]).unwrap())
                    .map(|_| arbitrary_type(g, size - 1))
                    .collect(),
                neg: (0..*g.choose(&[0, 1, 2]).unwrap())
                    .map(|_| arbitrary_type(g, size - 1))
                    .collect(),
            },
            _ => unreachable!(),
        }
    }
}

impl Arbitrary for Ty {
    fn arbitrary(g: &mut Gen) -> Ty {
        const MAX_SIZE: u32 = 2;
        arbitrary_type(g, MAX_SIZE)
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        match self.clone() {
            Ty::Union(types) => Box::new(types.shrink().filter_map(|elts| match elts.len() {
                0 => None,
                1 => Some(elts.into_iter().next().unwrap()),
                _ => Some(Ty::Union(elts)),
            })),
            Ty::Tuple(types) => Box::new(types.shrink().filter_map(|elts| match elts.len() {
                0 => None,
                1 => Some(elts.into_iter().next().unwrap()),
                _ => Some(Ty::Tuple(elts)),
            })),
            Ty::Intersection { pos, neg } => {
                // Shrinking on intersections is not exhaustive!
                //
                // We try to shrink the positive side or the negative side,
                // but we aren't shrinking both at the same time.
                //
                // This should remove positive or negative constraints but
                // won't shrink (A & B & ~C & ~D) to (A & ~C) in one shrink
                // iteration.
                //
                // Instead, it hopes that (A & B & ~C) or (A & ~C & ~D) fails
                // so that shrinking can happen there.
                let pos_orig = pos.clone();
                let neg_orig = neg.clone();
                Box::new(
                    // we shrink negative constraints first, as
                    // intersections with only negative constraints are
                    // more confusing
                    neg.shrink()
                        .map(move |shrunk_neg| Ty::Intersection {
                            pos: pos_orig.clone(),
                            neg: shrunk_neg,
                        })
                        .chain(pos.shrink().map(move |shrunk_pos| Ty::Intersection {
                            pos: shrunk_pos,
                            neg: neg_orig.clone(),
                        }))
                        .filter_map(|ty| {
                            if let Ty::Intersection { pos, neg } = &ty {
                                match (pos.len(), neg.len()) {
                                    // an empty intersection does not mean
                                    // anything
                                    (0, 0) => None,
                                    // a single positive element should be
                                    // unwrapped
                                    (1, 0) => Some(pos[0].clone()),
                                    _ => Some(ty),
                                }
                            } else {
                                unreachable!()
                            }
                        }),
                )
            }
            _ => Box::new(std::iter::empty()),
        }
    }
}

pub(crate) fn intersection<'db>(
    db: &'db TestDb,
    tys: impl IntoIterator<Item = Type<'db>>,
) -> Type<'db> {
    let mut builder = IntersectionBuilder::new(db);
    for ty in tys {
        builder = builder.add_positive(ty);
    }
    builder.build()
}

pub(crate) fn union<'db>(db: &'db TestDb, tys: impl IntoIterator<Item = Type<'db>>) -> Type<'db> {
    UnionType::from_elements(db, tys)
}
