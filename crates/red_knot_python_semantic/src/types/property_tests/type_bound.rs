use crate::db::tests::TestDb;
use crate::types::property_tests::type_generation::Ty;
use crate::types::KnownClass;
use quickcheck::{Arbitrary, Gen};
use std::{fmt, sync::OnceLock};

use super::setup::get_cached_db;

/// A wrapper for a type that implements the `quickcheck::Arbitrary` trait.
/// This struct enables the generation of arbitrary types for property test.
#[derive(Clone)]
pub(crate) struct BoundedType<T: TypeBound> {
    pub ty: Ty,
    _marker: std::marker::PhantomData<T>,
}

impl<Bound: TypeBound> BoundedType<Bound> {
    fn new(ty: Ty) -> Self {
        BoundedType {
            ty,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Bound: TypeBound + Clone + 'static> Arbitrary for BoundedType<Bound> {
    fn arbitrary(g: &mut Gen) -> Self {
        const MAX_SIZE: u32 = 2;
        BoundedType::new(Bound::generate_type_recursively(g, MAX_SIZE))
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(Bound::shrink(&self.ty).map(BoundedType::new))
    }
}

impl<Bound: TypeBound> fmt::Debug for BoundedType<Bound> {
    // for quickcheck to print the type name
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.ty)
    }
}

/// Note that `Vec<Ty>` don't have a `shrink` implementation, so we need to wrap each type
/// in a `BoundedType` and apply its `shrink` implementation.
fn shrink_types<Bound>(types: Vec<Ty>) -> Box<dyn Iterator<Item = Vec<Ty>>>
where
    Bound: TypeBound,
    BoundedType<Bound>: Arbitrary,
{
    let wrapped_vec: Vec<BoundedType<Bound>> =
        types.into_iter().map(BoundedType::<Bound>::new).collect();

    Box::new(
        wrapped_vec
            .shrink()
            .map(|shrunk| shrunk.into_iter().map(|wrapped_ty| wrapped_ty.ty).collect()),
    )
}

pub(crate) trait TypeBound: Clone + 'static {
    fn type_pool() -> &'static [Ty];

    /// Generate an arbitrary singular type only.
    /// This is a type that is not a Union, Tuple, or Intersection
    fn generate_singular_type(g: &mut Gen) -> Ty {
        let pool = Self::type_pool();
        g.choose(pool).unwrap().clone()
    }

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

static TYPE_POOL: &[Ty] = &[
    Ty::Never,
    Ty::Unknown,
    Ty::None,
    Ty::Any,
    Ty::IntLiteral(0),
    Ty::IntLiteral(1),
    Ty::IntLiteral(-1),
    Ty::BooleanLiteral(true),
    Ty::BooleanLiteral(false),
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
];

fn filter_type_pool(predicate: fn(db: &TestDb, ty: &Ty) -> bool) -> &'static [Ty] {
    let db = get_cached_db();
    let pool = TYPE_POOL
        .iter()
        .filter(|ty| predicate(&db, ty))
        .cloned()
        .collect::<Vec<Ty>>();

    assert!(!pool.is_empty(), "No types found in the pool");

    Box::leak(pool.into_boxed_slice())
}

#[derive(Debug, Clone)]
pub(crate) struct AnyTy;

impl TypeBound for AnyTy {
    fn type_pool() -> &'static [Ty] {
        TYPE_POOL
    }
}

/// A type that satisfy `is_fully_static`.
#[derive(Debug, Clone)]
pub(crate) struct FullyStaticTy;

static FULLY_STATIC_TYPE_POOL: OnceLock<&[Ty]> = OnceLock::new();
impl TypeBound for FullyStaticTy {
    fn type_pool() -> &'static [Ty] {
        FULLY_STATIC_TYPE_POOL
            .get_or_init(|| filter_type_pool(|db, ty| ty.clone().into_type(db).is_fully_static(db)))
    }
}

/// A type that satisfy `is_singleton`.
#[derive(Debug, Clone)]
pub(crate) struct SingletonTy;

static SINGLETON_TYPE_POOL: OnceLock<&[Ty]> = OnceLock::new();
impl TypeBound for SingletonTy {
    fn type_pool() -> &'static [Ty] {
        SINGLETON_TYPE_POOL
            .get_or_init(|| filter_type_pool(|db, ty| ty.clone().into_type(db).is_singleton(db)))
    }

    fn generate_type_recursively(g: &mut Gen, _: u32) -> Ty {
        // Tuple, Union, and Intersection are not allowed for SingletonTy
        Self::generate_singular_type(g)
    }
}
