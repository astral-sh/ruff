use crate::Db;
use crate::db::tests::TestDb;
use crate::place::{builtins_symbol, known_module_symbol};
use crate::types::enums::is_single_member_enum;
use crate::types::tuple::TupleType;
use crate::types::{
    BoundMethodType, EnumLiteralType, IntersectionBuilder, KnownClass, Parameter, Parameters,
    Signature, SpecialFormType, SubclassOfType, Type, UnionType,
};
use quickcheck::{Arbitrary, Gen};
use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use ty_module_resolver::KnownModule;

/// A test representation of a type that can be transformed unambiguously into a real Type,
/// given a db.
///
/// TODO: We should add some variants that exercise generic classes and specializations thereof.
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
    // An enum literal variant, using `uuid.SafeUUID` as base
    EnumLiteral(&'static str),
    // A single-member enum literal, using `dataclasses.MISSING`
    SingleMemberEnumLiteral,
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
    FixedLengthTuple(Vec<Ty>),
    VariableLengthTuple(Vec<Ty>, Box<Ty>, Vec<Ty>),
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
    Callable {
        params: CallableParams,
        returns: Option<Box<Ty>>,
    },
    /// `unittest.mock.Mock` is interesting because it is a nominal instance type
    /// where the class has `Any` in its MRO
    UnittestMockInstance,
    UnittestMockLiteral,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CallableParams {
    GradualForm,
    List(Vec<Param>),
}

impl CallableParams {
    pub(crate) fn into_parameters(self, db: &TestDb) -> Parameters<'_> {
        match self {
            CallableParams::GradualForm => Parameters::gradual_form(),
            CallableParams::List(params) => Parameters::new(
                db,
                params.into_iter().map(|param| {
                    let mut parameter = match param.kind {
                        ParamKind::PositionalOnly => Parameter::positional_only(param.name),
                        ParamKind::PositionalOrKeyword => {
                            Parameter::positional_or_keyword(param.name.unwrap())
                        }
                        ParamKind::Variadic => Parameter::variadic(param.name.unwrap()),
                        ParamKind::KeywordOnly => Parameter::keyword_only(param.name.unwrap()),
                        ParamKind::KeywordVariadic => {
                            Parameter::keyword_variadic(param.name.unwrap())
                        }
                    };
                    if let Some(annotated_ty) = param.annotated_ty {
                        parameter = parameter.with_annotated_type(annotated_ty.into_type(db));
                    }
                    if let Some(default_ty) = param.default_ty {
                        parameter = parameter.with_default_type(default_ty.into_type(db));
                    }
                    parameter
                }),
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Param {
    kind: ParamKind,
    name: Option<Name>,
    annotated_ty: Option<Ty>,
    default_ty: Option<Ty>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ParamKind {
    PositionalOnly,
    PositionalOrKeyword,
    Variadic,
    KeywordOnly,
    KeywordVariadic,
}

#[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
fn create_bound_method<'db>(
    db: &'db dyn Db,
    function: Type<'db>,
    builtins_class: Type<'db>,
) -> Type<'db> {
    Type::BoundMethod(BoundMethodType::new(
        db,
        function.expect_function_literal(),
        builtins_class.to_instance(db).unwrap(),
    ))
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
            Ty::EnumLiteral(name) => Type::EnumLiteral(EnumLiteralType::new(
                db,
                known_module_symbol(db, KnownModule::Uuid, "SafeUUID")
                    .place
                    .expect_type()
                    .expect_class_literal(),
                Name::new(name),
            )),
            Ty::SingleMemberEnumLiteral => {
                let ty = known_module_symbol(db, KnownModule::Dataclasses, "MISSING")
                    .place
                    .expect_type();
                debug_assert!(
                    matches!(ty, Type::NominalInstance(instance) if is_single_member_enum(db, instance.class_literal(db)))
                );
                ty
            }
            Ty::BuiltinInstance(s) => builtins_symbol(db, s)
                .place
                .expect_type()
                .to_instance(db)
                .unwrap(),
            Ty::AbcInstance(s) => known_module_symbol(db, KnownModule::Abc, s)
                .place
                .expect_type()
                .to_instance(db)
                .unwrap(),
            Ty::AbcClassLiteral(s) => known_module_symbol(db, KnownModule::Abc, s)
                .place
                .expect_type(),
            Ty::UnittestMockLiteral => known_module_symbol(db, KnownModule::UnittestMock, "Mock")
                .place
                .expect_type(),
            Ty::UnittestMockInstance => Ty::UnittestMockLiteral
                .into_type(db)
                .to_instance(db)
                .unwrap(),
            Ty::TypingLiteral => Type::SpecialForm(SpecialFormType::Literal),
            Ty::BuiltinClassLiteral(s) => builtins_symbol(db, s).place.expect_type(),
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
            Ty::FixedLengthTuple(tys) => {
                let elements = tys.into_iter().map(|ty| ty.into_type(db));
                Type::heterogeneous_tuple(db, elements)
            }
            Ty::VariableLengthTuple(prefix, variable, suffix) => {
                let prefix = prefix.into_iter().map(|ty| ty.into_type(db));
                let variable = variable.into_type(db);
                let suffix = suffix.into_iter().map(|ty| ty.into_type(db));
                Type::tuple(TupleType::mixed(db, prefix, variable, suffix))
            }
            Ty::SubclassOfAny => SubclassOfType::subclass_of_any(),
            Ty::SubclassOfBuiltinClass(s) => SubclassOfType::from(
                db,
                builtins_symbol(db, s)
                    .place
                    .expect_type()
                    .expect_class_literal()
                    .default_specialization(db),
            ),
            Ty::SubclassOfAbcClass(s) => SubclassOfType::from(
                db,
                known_module_symbol(db, KnownModule::Abc, s)
                    .place
                    .expect_type()
                    .expect_class_literal()
                    .default_specialization(db),
            ),
            Ty::AlwaysTruthy => Type::AlwaysTruthy,
            Ty::AlwaysFalsy => Type::AlwaysFalsy,
            Ty::BuiltinsFunction(name) => builtins_symbol(db, name).place.expect_type(),
            Ty::BuiltinsBoundMethod { class, method } => {
                let builtins_class = builtins_symbol(db, class).place.expect_type();
                let function = builtins_class.member(db, method).place.expect_type();

                create_bound_method(db, function, builtins_class)
            }
            Ty::Callable { params, returns } => Type::single_callable(
                db,
                Signature::new(
                    params.into_parameters(db),
                    returns.map(|ty| ty.into_type(db)),
                ),
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FullyStaticTy(Ty);

impl FullyStaticTy {
    pub(crate) fn into_type(self, db: &TestDb) -> Type<'_> {
        self.0.into_type(db)
    }
}

fn arbitrary_core_type(g: &mut Gen, fully_static: bool) -> Ty {
    // We could select a random integer here, but this would make it much less
    // likely to explore interesting edge cases:
    let int_lit = Ty::IntLiteral(*g.choose(&[-2, -1, 0, 1, 2]).unwrap());
    let bool_lit = Ty::BooleanLiteral(bool::arbitrary(g));

    // Update this if new non-fully-static types are added below.
    let fully_static_index = 5;
    let types = &[
        Ty::Any,
        Ty::Unknown,
        Ty::SubclassOfAny,
        Ty::UnittestMockLiteral,
        Ty::UnittestMockInstance,
        // Add fully static types below, dynamic types above.
        // Update `fully_static_index` above if adding new dynamic types!
        Ty::Never,
        Ty::None,
        int_lit,
        bool_lit,
        Ty::StringLiteral(""),
        Ty::StringLiteral("a"),
        Ty::LiteralString,
        Ty::BytesLiteral(""),
        Ty::BytesLiteral("\x00"),
        Ty::EnumLiteral("safe"),
        Ty::EnumLiteral("unsafe"),
        Ty::EnumLiteral("unknown"),
        Ty::SingleMemberEnumLiteral,
        Ty::KnownClassInstance(KnownClass::Object),
        Ty::KnownClassInstance(KnownClass::Str),
        Ty::KnownClassInstance(KnownClass::Int),
        Ty::KnownClassInstance(KnownClass::Bool),
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
    ];
    let types = if fully_static {
        &types[fully_static_index..]
    } else {
        types
    };
    g.choose(types).unwrap().clone()
}

/// Constructs an arbitrary type.
///
/// The `size` parameter controls the depth of the type tree. For example,
/// a simple type like `int` has a size of 0, `Union[int, str]` has a size
/// of 1, `tuple[int, Union[str, bytes]]` has a size of 2, etc.
///
/// The `fully_static` parameter, if `true`, limits generation to fully static types.
fn arbitrary_type(g: &mut Gen, size: u32, fully_static: bool) -> Ty {
    if size == 0 {
        arbitrary_core_type(g, fully_static)
    } else {
        match u32::arbitrary(g) % 6 {
            0 => arbitrary_core_type(g, fully_static),
            1 => Ty::Union(
                (0..*g.choose(&[2, 3]).unwrap())
                    .map(|_| arbitrary_type(g, size - 1, fully_static))
                    .collect(),
            ),
            2 => Ty::FixedLengthTuple(
                (0..*g.choose(&[0, 1, 2]).unwrap())
                    .map(|_| arbitrary_type(g, size - 1, fully_static))
                    .collect(),
            ),
            3 => Ty::VariableLengthTuple(
                (0..*g.choose(&[0, 1, 2]).unwrap())
                    .map(|_| arbitrary_type(g, size - 1, fully_static))
                    .collect(),
                Box::new(arbitrary_type(g, size - 1, fully_static)),
                (0..*g.choose(&[0, 1, 2]).unwrap())
                    .map(|_| arbitrary_type(g, size - 1, fully_static))
                    .collect(),
            ),
            4 => Ty::Intersection {
                pos: (0..*g.choose(&[0, 1, 2]).unwrap())
                    .map(|_| arbitrary_type(g, size - 1, fully_static))
                    .collect(),
                neg: (0..*g.choose(&[0, 1, 2]).unwrap())
                    .map(|_| arbitrary_type(g, size - 1, fully_static))
                    .collect(),
            },
            5 => Ty::Callable {
                params: match u32::arbitrary(g) % 2 {
                    0 if !fully_static => CallableParams::GradualForm,
                    _ => CallableParams::List(arbitrary_parameter_list(g, size, fully_static)),
                },
                returns: arbitrary_annotation(g, size - 1, fully_static).map(Box::new),
            },
            _ => unreachable!(),
        }
    }
}

fn arbitrary_parameter_list(g: &mut Gen, size: u32, fully_static: bool) -> Vec<Param> {
    let mut params: Vec<Param> = vec![];
    let mut used_names = FxHashSet::default();

    // First, choose the number of parameters to generate.
    for _ in 0..*g.choose(&[0, 1, 2, 3, 4, 5]).unwrap() {
        // Next, choose the kind of parameters that can be generated based on the last parameter.
        let next_kind = match params.last().map(|p| p.kind) {
            None | Some(ParamKind::PositionalOnly) => *g
                .choose(&[
                    ParamKind::PositionalOnly,
                    ParamKind::PositionalOrKeyword,
                    ParamKind::Variadic,
                    ParamKind::KeywordOnly,
                    ParamKind::KeywordVariadic,
                ])
                .unwrap(),
            Some(ParamKind::PositionalOrKeyword) => *g
                .choose(&[
                    ParamKind::PositionalOrKeyword,
                    ParamKind::Variadic,
                    ParamKind::KeywordOnly,
                    ParamKind::KeywordVariadic,
                ])
                .unwrap(),
            Some(ParamKind::Variadic | ParamKind::KeywordOnly) => *g
                .choose(&[ParamKind::KeywordOnly, ParamKind::KeywordVariadic])
                .unwrap(),
            Some(ParamKind::KeywordVariadic) => {
                // There can't be any other parameter kind after a keyword variadic parameter.
                break;
            }
        };

        let name = loop {
            let name = if matches!(next_kind, ParamKind::PositionalOnly) {
                arbitrary_optional_name(g)
            } else {
                Some(arbitrary_name(g))
            };
            if let Some(name) = name {
                if used_names.insert(name.clone()) {
                    break Some(name);
                }
            } else {
                break None;
            }
        };

        params.push(Param {
            kind: next_kind,
            name,
            annotated_ty: arbitrary_annotation(g, size, fully_static),
            default_ty: if matches!(next_kind, ParamKind::Variadic | ParamKind::KeywordVariadic) {
                None
            } else {
                arbitrary_optional_type(g, size, fully_static)
            },
        });
    }

    params
}

/// An arbitrary optional type, always `Some` if fully static.
fn arbitrary_annotation(g: &mut Gen, size: u32, fully_static: bool) -> Option<Ty> {
    if fully_static {
        Some(arbitrary_type(g, size, true))
    } else {
        arbitrary_optional_type(g, size, false)
    }
}

fn arbitrary_optional_type(g: &mut Gen, size: u32, fully_static: bool) -> Option<Ty> {
    match u32::arbitrary(g) % 2 {
        0 => None,
        1 => Some(arbitrary_type(g, size, fully_static)),
        _ => unreachable!(),
    }
}

fn arbitrary_name(g: &mut Gen) -> Name {
    Name::new(format!("n{}", u32::arbitrary(g) % 10))
}

fn arbitrary_optional_name(g: &mut Gen) -> Option<Name> {
    match u32::arbitrary(g) % 2 {
        0 => None,
        1 => Some(arbitrary_name(g)),
        _ => unreachable!(),
    }
}

impl Arbitrary for Ty {
    fn arbitrary(g: &mut Gen) -> Ty {
        const MAX_SIZE: u32 = 2;
        arbitrary_type(g, MAX_SIZE, false)
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        match self.clone() {
            Ty::Union(types) => Box::new(types.shrink().filter_map(|elts| match elts.len() {
                0 => None,
                1 => Some(elts.into_iter().next().unwrap()),
                _ => Some(Ty::Union(elts)),
            })),
            Ty::FixedLengthTuple(types) => {
                Box::new(types.shrink().filter_map(|elts| match elts.len() {
                    0 => None,
                    1 => Some(elts.into_iter().next().unwrap()),
                    _ => Some(Ty::FixedLengthTuple(elts)),
                }))
            }
            Ty::VariableLengthTuple(prefix, variable, suffix) => {
                // We shrink the suffix first, then the prefix, then the variable-length type.
                let suffix_shrunk = suffix.shrink().map({
                    let prefix = prefix.clone();
                    let variable = variable.clone();
                    move |suffix| Ty::VariableLengthTuple(prefix.clone(), variable.clone(), suffix)
                });
                let prefix_shrunk = prefix.shrink().map({
                    let variable = variable.clone();
                    let suffix = suffix.clone();
                    move |prefix| Ty::VariableLengthTuple(prefix, variable.clone(), suffix.clone())
                });
                let variable_shrunk = variable.shrink().map({
                    let prefix = prefix.clone();
                    let suffix = suffix.clone();
                    move |variable| {
                        Ty::VariableLengthTuple(prefix.clone(), variable, suffix.clone())
                    }
                });
                Box::new(suffix_shrunk.chain(prefix_shrunk).chain(variable_shrunk))
            }
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

impl Arbitrary for FullyStaticTy {
    fn arbitrary(g: &mut Gen) -> FullyStaticTy {
        const MAX_SIZE: u32 = 2;
        FullyStaticTy(arbitrary_type(g, MAX_SIZE, true))
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(self.0.shrink().map(FullyStaticTy))
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
