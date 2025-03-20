use crate::types::property_tests::type_generation::Ty;
use crate::types::KnownClass;
use quickcheck::{Arbitrary, Gen};
use std::fmt;

/// A wrapper for a type that implements the `quickcheck::Arbitrary` trait.
/// This struct enables the generation of arbitrary types for property test.
#[derive(Clone)]
pub(crate) struct QuickcheckArgument<T: ArbitraryRule> {
    pub ty: Ty,
    _marker: std::marker::PhantomData<T>,
}

impl<Rule: ArbitraryRule + Clone + 'static> Arbitrary for QuickcheckArgument<Rule> {
    fn arbitrary(g: &mut Gen) -> Self {
        const MAX_SIZE: u32 = 2;
        QuickcheckArgument {
            ty: Rule::generate_type_recursively(g, MAX_SIZE),
            _marker: std::marker::PhantomData,
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(Rule::shrink(&self.ty).map(move |ty| QuickcheckArgument {
            ty,
            _marker: std::marker::PhantomData,
        }))
    }
}

impl<Rule: ArbitraryRule> fmt::Debug for QuickcheckArgument<Rule> {
    // for quickcheck to print the type name
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.ty)
    }
}

/// Note that `Vec<Ty>` don't have a `shrink` implementation, so we need to wrap each type
/// in a `QuickcheckArgument` and apply its `shrink` implementation.
fn shrink_types<Rule>(types: Vec<Ty>) -> Box<dyn Iterator<Item = Vec<Ty>>>
where
    Rule: ArbitraryRule,
    QuickcheckArgument<Rule>: Arbitrary,
{
    let wrapped_vec: Vec<QuickcheckArgument<Rule>> = types
        .into_iter()
        .map(|ty| QuickcheckArgument::<Rule> {
            ty,
            _marker: std::marker::PhantomData,
        })
        .collect();

    Box::new(
        wrapped_vec
            .shrink()
            .map(|shrunk| shrunk.into_iter().map(|wrapped_ty| wrapped_ty.ty).collect()),
    )
}

pub(crate) trait ArbitraryRule: Clone + 'static {
    /// Generate an arbitrary singular type only.
    /// This is a type that is not a Union, Tuple, or Intersection
    fn generate_singular_type(g: &mut Gen) -> Ty;

    /// Generate an arbitrary type recursively.
    ///
    /// The `size` parameter controls the depth of the type tree. For example,
    /// a simple type like `int` has a size of 0, `Union[int, str]` has a size
    /// of 1, `tuple[int, Union[str, bytes]]` has a size of 2, etc.
    fn generate_type_recursively(g: &mut Gen, size: u32) -> Ty {
        if size == 0 {
            Self::generate_singular_type(g)
        } else {
            match u32::arbitrary(g) % 4 {
                0 => Self::generate_singular_type(g),
                1 => Ty::Union(
                    (0..*g.choose(&[2, 3]).unwrap())
                        .map(|_| Self::generate_type_recursively(g, size - 1))
                        .collect(),
                ),
                2 => Ty::Tuple(
                    (0..*g.choose(&[0, 1, 2]).unwrap())
                        .map(|_| Self::generate_type_recursively(g, size - 1))
                        .collect(),
                ),
                3 => Ty::Intersection {
                    pos: (0..*g.choose(&[0, 1, 2]).unwrap())
                        .map(|_| Self::generate_type_recursively(g, size - 1))
                        .collect(),
                    neg: (0..*g.choose(&[0, 1, 2]).unwrap())
                        .map(|_| Self::generate_type_recursively(g, size - 1))
                        .collect(),
                },
                _ => unreachable!(),
            }
        }
    }

    fn shrink(ty: &Ty) -> Box<dyn Iterator<Item = Ty>> {
        match ty.clone() {
            Ty::Union(types) => {
                let shrunk_iter = shrink_types::<Self>(types);
                Box::new(shrunk_iter.filter_map(|shrunk| match shrunk.len() {
                    0 => None,
                    1 => Some(shrunk.into_iter().next().unwrap()),
                    _ => Some(Ty::Union(shrunk)),
                }))
            }
            Ty::Tuple(types) => {
                let shrunk_iter = shrink_types::<Self>(types);
                Box::new(shrunk_iter.filter_map(|shrunk| match shrunk.len() {
                    0 => None,
                    1 => Some(shrunk.into_iter().next().unwrap()),
                    _ => Some(Ty::Tuple(shrunk)),
                }))
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
                let shrunk_pos_iter = shrink_types::<Self>(pos);
                let shrunk_neg_iter = shrink_types::<Self>(neg);
                Box::new(
                    // we shrink negative constraints first, as
                    // intersections with only negative constraints are
                    // more confusing
                    shrunk_neg_iter
                        .map(move |shrunk_neg| Ty::Intersection {
                            pos: pos_orig.clone(),
                            neg: shrunk_neg,
                        })
                        .chain(shrunk_pos_iter.map(move |shrunk_pos| Ty::Intersection {
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

/// A constraint that allows any type.
#[derive(Debug, Clone)]
pub(crate) struct AnyTy;

impl ArbitraryRule for AnyTy {
    fn generate_singular_type(g: &mut Gen) -> Ty {
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
}

/// A constraint that passes `t.is_fully_static()`.
#[derive(Debug, Clone)]
pub(crate) struct FullyStaticTy;

impl ArbitraryRule for FullyStaticTy {
    fn generate_singular_type(g: &mut Gen) -> Ty {
        let int_lit = Ty::IntLiteral(*g.choose(&[-2, -1, 0, 1, 2]).unwrap());
        let bool_lit = Ty::BooleanLiteral(bool::arbitrary(g));
        g.choose(&[
            Ty::Never,
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
            Ty::AlwaysFalsy,
            Ty::AlwaysTruthy,
        ])
        .unwrap()
        .clone()
    }
}

/// A constraint that passes `t.is_singleton()`
#[derive(Debug, Clone)]
pub(crate) struct SingletonTy;

impl ArbitraryRule for SingletonTy {
    fn generate_singular_type(g: &mut Gen) -> Ty {
        g.choose(&[
            Ty::BooleanLiteral(true),
            Ty::BooleanLiteral(false),
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
            Ty::KnownClassInstance(KnownClass::NoneType),
            Ty::KnownClassInstance(KnownClass::EllipsisType),
            Ty::KnownClassInstance(KnownClass::NoDefaultType),
            Ty::KnownClassInstance(KnownClass::VersionInfo),
            Ty::KnownClassInstance(KnownClass::TypeAliasType),
        ])
        .unwrap()
        .clone()
    }

    fn generate_type_recursively(g: &mut Gen, _: u32) -> Ty {
        // Tuple, Union, and Intersection are not allowed for SingletonTy
        Self::generate_singular_type(g)
    }
}
