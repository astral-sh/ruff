use crate::Db;
use crate::place::{DefinedPlace, Place, builtins_symbol, global_symbol, known_module_symbol};
use crate::types::enums::is_single_member_enum;
use crate::types::known_instance::KnownInstanceType;
use crate::types::tuple::TupleType;
use crate::types::{
    ApplyTypeMappingVisitor, BoundMethodType, EnumLiteralType, IntersectionBuilder,
    IntersectionType, KnownClass, MaterializationKind, Parameter, Parameters, Signature,
    SpecialFormType, SubclassOfType, Type, UnionType,
};
use crate::{Program, ProgramEnvironment};
use itertools::Either;
use quickcheck::{Arbitrary, Gen};
use ruff_db::files::system_path_to_file;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use ty_module_resolver::KnownModule;
use ty_python_core::ProgramFile;

/// A test representation of a type that can be transformed unambiguously into a real Type,
/// given a db.
///
/// TODO: We should add some variants that exercise generic classes and specializations thereof.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Ty {
    Never,
    Unknown,
    Divergent,
    TopDivergent,
    BottomDivergent,
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
        returns: Box<Ty>,
    },
    /// `unittest.mock.Mock` is interesting because it is a nominal instance type
    /// where the class has `Any` in its MRO
    UnittestMockInstance,
    UnittestMockLiteral,
    /// Instances of various `NewType`s that we construct in `setup.rs`.
    /// `FloatNewType` and `ComplexNewType` are interesting because they are the only
    /// kinds of `NewType`s that can have unions as their concrete base types.
    IntNewtypeInstance,
    StrNewtypeInstance,
    FloatNewtypeInstance,
    ComplexNewtypeInstance,
    SubNewTypeOfIntInstance,
    SubSubNewTypeOfIntInstance,
    SubNewTypeOfFloatInstance,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CallableParams {
    GradualForm,
    List(Vec<Param>),
}

impl CallableParams {
    fn into_parameters<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Parameters<'db> {
        match self {
            CallableParams::GradualForm => Parameters::gradual_form(),
            CallableParams::List(params) => Parameters::from_annotation(
                db,
                params.into_iter().map(|param| {
                    let parameter = match param.kind {
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
                    parameter
                        .with_annotated_type(param.annotated_ty.into_type(db, env))
                        .with_optional_default_type(param.default_ty.map(|t| t.into_type(db, env)))
                }),
            ),
        }
    }

    fn shrink(self) -> impl Iterator<Item = Self> {
        match self {
            // If the failure does not depend on accepting arbitrary arguments, replace `...`
            // with the simplest concrete signature: one that accepts no arguments.
            Self::GradualForm => Either::Left(std::iter::once(Self::List(Vec::new()))),
            Self::List(params) => {
                // Removing one parameter at a time preserves the ordering and names of all
                // remaining parameters, so each candidate is still a valid signature.
                let removed_parameters = (0..params.len()).map({
                    let params = params.clone();
                    move |index| {
                        let mut shrunk = params.clone();
                        shrunk.remove(index);
                        Self::List(shrunk)
                    }
                });

                // If a parameter cannot be removed without losing the failure, try simplifying its
                // name, default, or annotation while preserving the rest of the signature.
                let shrunk_parameters = (0..params.len()).flat_map(move |index| {
                    let params = params.clone();
                    params[index].clone().shrink().map(move |parameter| {
                        let mut shrunk = params.clone();
                        shrunk[index] = parameter;
                        Self::List(shrunk)
                    })
                });

                Either::Right(removed_parameters.chain(shrunk_parameters))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Param {
    kind: ParamKind,
    name: Option<Name>,
    annotated_ty: Ty,
    default_ty: Option<Ty>,
}

impl Param {
    fn shrink(self) -> impl Iterator<Item = Self> {
        let without_name =
            (self.kind == ParamKind::PositionalOnly && self.name.is_some()).then(|| Self {
                name: None,
                ..self.clone()
            });

        let shrunk_defaults = self.default_ty.shrink().map({
            let parameter = self.clone();
            move |default_ty| Self {
                default_ty,
                ..parameter.clone()
            }
        });

        let shrunk_annotations = shrink_callable_component(&self.annotated_ty).map({
            let parameter = self.clone();
            move |annotated_ty| Self {
                annotated_ty,
                ..parameter.clone()
            }
        });

        without_name
            .into_iter()
            .chain(shrunk_defaults)
            .chain(shrunk_annotations)
    }
}

fn shrink_callable_component(ty: &Ty) -> impl Iterator<Item = Ty> + use<> {
    let object = Ty::KnownClassInstance(KnownClass::Object);
    let simplified = (ty != &object).then_some(object);

    simplified.into_iter().chain(ty.shrink())
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ParamKind {
    PositionalOnly,
    PositionalOrKeyword,
    Variadic,
    KeywordOnly,
    KeywordVariadic,
}

#[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
fn create_bound_method<'db>(
    db: &'db dyn Db,
    program: Program<'db>,
    function: Type<'db>,
    builtins_class: Type<'db>,
) -> Type<'db> {
    let env = ProgramEnvironment::from_program(program);
    let self_instance = builtins_class.to_instance_approximation(db, &env).unwrap();
    Type::BoundMethod(BoundMethodType::new(
        db,
        function.expect_function_literal(),
        self_instance,
        self_instance,
    ))
}

impl Ty {
    pub(crate) fn into_type<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Type<'db> {
        match self {
            Ty::Never => Type::Never,
            Ty::Unknown => Type::unknown(),
            Ty::Divergent => divergent(db, env, 1, None),
            Ty::TopDivergent => divergent(db, env, 2, Some(MaterializationKind::Top)),
            Ty::BottomDivergent => divergent(db, env, 3, Some(MaterializationKind::Bottom)),
            Ty::None => Type::none(db, env),
            Ty::Any => Type::any(),
            Ty::IntLiteral(n) => Type::int_literal(n),
            Ty::StringLiteral(s) => Type::string_literal(db, s),
            Ty::BooleanLiteral(b) => Type::bool_literal(b),
            Ty::LiteralString => Type::literal_string(),
            Ty::BytesLiteral(s) => Type::bytes_literal(db, s.as_bytes()),
            Ty::EnumLiteral(name) => {
                let enum_class = known_module_symbol(db, env, KnownModule::Uuid, "SafeUUID")
                    .place
                    .expect_type()
                    .expect_class_literal()
                    .into_enum_class(db)
                    .expect("`uuid.SafeUUID` is an enum");
                Type::enum_literal(EnumLiteralType::new(db, enum_class, Name::new(name)))
            }
            Ty::SingleMemberEnumLiteral => {
                let ty = known_module_symbol(db, env, KnownModule::Dataclasses, "MISSING")
                    .place
                    .expect_type();
                debug_assert!(
                    matches!(ty, Type::NominalInstance(instance) if is_single_member_enum(db, instance.class_literal(db, env)))
                );
                ty
            }
            Ty::BuiltinInstance(s) => builtins_symbol(db, env, s)
                .place
                .expect_type()
                .to_instance_approximation(db, env)
                .unwrap(),
            Ty::AbcInstance(s) => known_module_symbol(db, env, KnownModule::Abc, s)
                .place
                .expect_type()
                .to_instance_approximation(db, env)
                .unwrap(),
            Ty::AbcClassLiteral(s) => known_module_symbol(db, env, KnownModule::Abc, s)
                .place
                .expect_type(),
            Ty::UnittestMockLiteral => {
                known_module_symbol(db, env, KnownModule::UnittestMock, "Mock")
                    .place
                    .expect_type()
            }
            Ty::UnittestMockInstance => Ty::UnittestMockLiteral
                .into_type(db, env)
                .to_instance_approximation(db, env)
                .unwrap(),
            Ty::TypingLiteral => Type::SpecialForm(SpecialFormType::Literal),
            Ty::BuiltinClassLiteral(s) => builtins_symbol(db, env, s).place.expect_type(),
            Ty::KnownClassInstance(known_class) => known_class.to_instance(db, env),
            Ty::Union(tys) => {
                UnionType::from_elements(db, env, tys.into_iter().map(|ty| ty.into_type(db, env)))
            }
            Ty::Intersection { pos, neg } => {
                let mut builder = IntersectionBuilder::new(db, env);
                for p in pos {
                    builder.add_positive_in_place(p.into_type(db, env));
                }
                for n in neg {
                    builder.add_negative_in_place(n.into_type(db, env));
                }
                builder.build()
            }
            Ty::FixedLengthTuple(tys) => {
                let elements = tys.into_iter().map(|ty| ty.into_type(db, env));
                Type::heterogeneous_tuple(db, env, elements)
            }
            Ty::VariableLengthTuple(prefix, variable, suffix) => {
                let prefix = prefix.into_iter().map(|ty| ty.into_type(db, env));
                let variable = variable.into_type(db, env);
                let suffix = suffix.into_iter().map(|ty| ty.into_type(db, env));
                Type::tuple(TupleType::mixed(db, env, prefix, variable, suffix))
            }
            Ty::SubclassOfAny => SubclassOfType::subclass_of_any(),
            Ty::SubclassOfBuiltinClass(s) => SubclassOfType::from(
                db,
                env,
                builtins_symbol(db, env, s)
                    .place
                    .expect_type()
                    .expect_class_literal()
                    .default_specialization(db),
            ),
            Ty::SubclassOfAbcClass(s) => SubclassOfType::from(
                db,
                env,
                known_module_symbol(db, env, KnownModule::Abc, s)
                    .place
                    .expect_type()
                    .expect_class_literal()
                    .default_specialization(db),
            ),
            Ty::AlwaysTruthy => Type::AlwaysTruthy,
            Ty::AlwaysFalsy => Type::AlwaysFalsy,
            Ty::BuiltinsFunction(name) => builtins_symbol(db, env, name).place.expect_type(),
            Ty::BuiltinsBoundMethod { class, method } => {
                let builtins_class = builtins_symbol(db, env, class).place.expect_type();
                let function = builtins_class.member(db, env, method).place.expect_type();

                create_bound_method(db, env.program(db), function, builtins_class)
            }
            Ty::Callable { params, returns } => Type::single_callable(
                db,
                Signature::new(params.into_parameters(db, env), returns.into_type(db, env)),
            ),
            Ty::FloatNewtypeInstance => newtype_instance(db, env, "NewTypeOfFloat"),
            Ty::IntNewtypeInstance => newtype_instance(db, env, "NewTypeOfInt"),
            Ty::StrNewtypeInstance => newtype_instance(db, env, "NewTypeOfStr"),
            Ty::ComplexNewtypeInstance => newtype_instance(db, env, "NewTypeOfComplex"),
            Ty::SubNewTypeOfIntInstance => newtype_instance(db, env, "SubNewTypeOfInt"),
            Ty::SubSubNewTypeOfIntInstance => newtype_instance(db, env, "SubSubNewTypeOfInt"),
            Ty::SubNewTypeOfFloatInstance => newtype_instance(db, env, "SubNewTypeOfFloat"),
        }
    }
}

fn divergent<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    id_bits: u64,
    materialization: Option<MaterializationKind>,
) -> Type<'db> {
    let divergent = Type::divergent(salsa::plumbing::Id::from_bits(id_bits));

    match materialization {
        Some(materialization_kind) => {
            divergent.materialize(db, materialization_kind, &ApplyTypeMappingVisitor::new(env))
        }
        None => divergent,
    }
}

fn newtype_instance<'db>(db: &'db dyn Db, env: &ProgramEnvironment<'db>, name: &str) -> Type<'db> {
    let file = system_path_to_file(db, super::setup::PROPERTY_TEST_MODULE_PATH)
        .expect("Property-test module must exist");
    let file = ProgramFile::new(db, file, env.program(db));
    let Place::Defined(DefinedPlace { ty, .. }) = global_symbol(db, file, name).place else {
        panic!(
            "Expected a global symbol for `{name}` in the property test module, but it was not found"
        );
    };
    match ty {
        Type::KnownInstance(KnownInstanceType::NewType(newtype)) => Type::NewTypeInstance(newtype),
        _ => panic!("Expected NewType symbol for `{name}`, got {ty:?}"),
    }
}

/// A `QuickCheck` input generated without dynamic components, including in nested unions, tuples,
/// and callables.
///
/// Some type properties, such as reflexivity of subtyping, only hold for fully static types. It is
/// tempting to generate an arbitrary [`Ty`] and express such a property as an implication:
///
/// ```text
/// t.is_fully_static(db, env) => t.is_subtype_of(db, env, t)
/// ```
///
/// However, the property-test macro implements implications as `!premise || conclusion`. Every
/// non-static input therefore counts as a successful `QuickCheck` iteration even though the property
/// itself was never checked. If `QUICKCHECK_TESTS=100000`, the test can report 100,000 successful
/// iterations while checking reflexivity for far fewer types. Properties with two fully static
/// inputs lose even more coverage because both inputs must satisfy the premise.
///
/// Filtering also disproportionately removes nested unions, tuples, and callables: each additional
/// component gives the generated type another opportunity to contain a dynamic type. Generating
/// fully static components directly ensures that every `QuickCheck` iteration checks the property
/// and that complex types remain represented alongside simple ones.
///
/// See <https://github.com/astral-sh/ruff/pull/27693> for the discussion of this coverage problem.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FullyStaticTy(Ty);

impl FullyStaticTy {
    pub(crate) fn into_type<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Type<'db> {
        let ty = self.0.into_type(db, env);
        assert!(
            ty.is_fully_static(db, env),
            "FullyStaticTy generated a non-static type: {}",
            ty.display(db, env),
        );
        ty
    }
}

// A single draw across both groups keeps unrestricted candidates equally likely without
// allocating a combined list or maintaining a positional boundary between the groups.
macro_rules! choose_core_type {
    (
        $generator:expr,
        $fully_static:expr,
        dynamic_types: [$($dynamic:expr),+ $(,)?],
        fully_static_types: [$($static:expr),+ $(,)?] $(,)?
    ) => {{
        if $fully_static {
            $generator.choose(&[$($static),+]).unwrap().clone()
        } else {
            $generator
                .choose(&[$($dynamic),+, $($static),+])
                .unwrap()
                .clone()
        }
    }};
}

fn arbitrary_core_type(g: &mut Gen, fully_static: bool) -> Ty {
    // We could select a random integer here, but this would make it much less
    // likely to explore interesting edge cases:
    let int_lit = Ty::IntLiteral(*g.choose(&[-2, -1, 0, 1, 2]).unwrap());
    let bool_lit = Ty::BooleanLiteral(bool::arbitrary(g));

    choose_core_type!(
        g,
        fully_static,
        dynamic_types: [
            Ty::Any,
            Ty::Unknown,
            Ty::Divergent,
            Ty::SubclassOfAny,
            Ty::UnittestMockInstance,
        ],
        fully_static_types: [
            Ty::Never,
            Ty::TopDivergent,
            Ty::BottomDivergent,
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
            Ty::KnownClassInstance(KnownClass::Float),
            Ty::KnownClassInstance(KnownClass::Complex),
            Ty::KnownClassInstance(KnownClass::Bool),
            Ty::KnownClassInstance(KnownClass::FunctionType),
            Ty::KnownClassInstance(KnownClass::SpecialForm),
            Ty::KnownClassInstance(KnownClass::TypeVar),
            Ty::KnownClassInstance(KnownClass::ExtensionsTypeAliasType),
            Ty::KnownClassInstance(KnownClass::NoDefaultType),
            Ty::TypingLiteral,
            Ty::UnittestMockLiteral,
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
            Ty::IntNewtypeInstance,
            Ty::StrNewtypeInstance,
            Ty::FloatNewtypeInstance,
            Ty::ComplexNewtypeInstance,
            Ty::SubNewTypeOfIntInstance,
            Ty::SubSubNewTypeOfIntInstance,
            Ty::SubNewTypeOfFloatInstance,
        ],
    )
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
                returns: Box::new(arbitrary_type(g, size - 1, fully_static)),
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
            annotated_ty: arbitrary_type(g, size, fully_static),
            default_ty: if matches!(next_kind, ParamKind::Variadic | ParamKind::KeywordVariadic) {
                None
            } else {
                arbitrary_optional_type(g, size, fully_static)
            },
        });
    }

    params
}

fn arbitrary_optional_type(g: &mut Gen, size: u32, fully_static: bool) -> Option<Ty> {
    bool::arbitrary(g).then(|| arbitrary_type(g, size, fully_static))
}

fn arbitrary_name(g: &mut Gen) -> Name {
    Name::new(format!("n{}", u32::arbitrary(g) % 10))
}

fn arbitrary_optional_name(g: &mut Gen) -> Option<Name> {
    bool::arbitrary(g).then(|| arbitrary_name(g))
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
            Ty::Callable { params, returns } => {
                let shrunk_parameters = params.clone().shrink().map({
                    let returns = returns.clone();
                    move |params| Ty::Callable {
                        params,
                        returns: returns.clone(),
                    }
                });

                let shrunk_return_type =
                    shrink_callable_component(&returns).map(move |returns| Ty::Callable {
                        params: params.clone(),
                        returns: Box::new(returns),
                    });

                Box::new(shrunk_parameters.chain(shrunk_return_type))
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
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    tys: impl IntoIterator<Item = Type<'db>>,
) -> Type<'db> {
    IntersectionType::from_elements(db, env, tys)
}

pub(crate) fn union<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    tys: impl IntoIterator<Item = Type<'db>>,
) -> Type<'db> {
    UnionType::from_elements(db, env, tys)
}

mod tests {
    use super::*;
    use test_case::test_case;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum CallableShrink {
        Parameter,
        ParameterName,
        ParameterDefault,
        ParameterAnnotation,
        ReturnType,
    }

    // Test each independently removable signature detail separately so a failure identifies the
    // exact shrink candidate that is missing.
    #[test_case(CallableShrink::Parameter; "removes a parameter")]
    #[test_case(CallableShrink::ParameterName; "removes a positional only parameter name")]
    #[test_case(CallableShrink::ParameterDefault; "removes a parameter default")]
    #[test_case(CallableShrink::ParameterAnnotation; "simplifies a parameter annotation")]
    #[test_case(CallableShrink::ReturnType; "simplifies the return type")]
    fn callable_shrinks_parameters_and_return_type(shrink: CallableShrink) {
        let parameter = Param {
            kind: ParamKind::PositionalOnly,
            name: Some(Name::new_static("argument")),
            annotated_ty: Ty::Union(vec![Ty::KnownClassInstance(KnownClass::Int), Ty::None]),
            default_ty: Some(Ty::IntLiteral(1)),
        };
        let callable = Ty::Callable {
            params: CallableParams::List(vec![parameter.clone()]),
            returns: Box::new(Ty::FixedLengthTuple(vec![])),
        };

        let mut expected_parameters = vec![parameter];
        let mut expected_return = Ty::FixedLengthTuple(vec![]);
        match shrink {
            CallableShrink::Parameter => expected_parameters.clear(),
            CallableShrink::ParameterName => expected_parameters[0].name = None,
            CallableShrink::ParameterDefault => expected_parameters[0].default_ty = None,
            CallableShrink::ParameterAnnotation => {
                expected_parameters[0].annotated_ty = Ty::KnownClassInstance(KnownClass::Object);
            }
            CallableShrink::ReturnType => {
                expected_return = Ty::KnownClassInstance(KnownClass::Object);
            }
        }

        let expected = Ty::Callable {
            params: CallableParams::List(expected_parameters),
            returns: Box::new(expected_return),
        };
        assert!(callable.shrink().any(|candidate| candidate == expected));
    }

    // A gradual `...` parameter list can become an empty concrete signature when accepting
    // arbitrary arguments is not essential to the failing property.
    #[test]
    fn gradual_callable_shrinks_to_empty_parameter_list() {
        let callable = Ty::Callable {
            params: CallableParams::GradualForm,
            returns: Box::new(Ty::KnownClassInstance(KnownClass::Object)),
        };

        assert_eq!(
            callable.shrink().collect::<Vec<_>>(),
            vec![Ty::Callable {
                params: CallableParams::List(vec![]),
                returns: Box::new(Ty::KnownClassInstance(KnownClass::Object)),
            }]
        );
    }
}
