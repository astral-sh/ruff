//! Display implementations for types.

use std::fmt::{self, Display, Formatter, Write};

use ruff_db::display::FormatterJoinExtension;
use ruff_python_ast::str::Quote;
use ruff_python_literal::escape::AsciiEscape;

use crate::types::class_base::ClassBase;
use crate::types::{
    ClassLiteralType, InstanceType, IntersectionType, KnownClass, StringLiteralType,
    SubclassOfType, Type, UnionType,
};
use crate::Db;
use rustc_hash::FxHashMap;

impl<'db> Type<'db> {
    pub fn display(&self, db: &'db dyn Db) -> DisplayType {
        DisplayType { ty: self, db }
    }
    fn representation(self, db: &'db dyn Db) -> DisplayRepresentation<'db> {
        DisplayRepresentation { db, ty: self }
    }
}

#[derive(Copy, Clone)]
pub struct DisplayType<'db> {
    ty: &'db Type<'db>,
    db: &'db dyn Db,
}

impl Display for DisplayType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let representation = self.ty.representation(self.db);
        if matches!(
            self.ty,
            Type::IntLiteral(_)
                | Type::BooleanLiteral(_)
                | Type::StringLiteral(_)
                | Type::BytesLiteral(_)
                | Type::ClassLiteral(_)
                | Type::FunctionLiteral(_)
        ) {
            write!(f, "Literal[{representation}]")
        } else {
            representation.fmt(f)
        }
    }
}

impl fmt::Debug for DisplayType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)
    }
}

/// Writes the string representation of a type, which is the value displayed either as
/// `Literal[<repr>]` or `Literal[<repr1>, <repr2>]` for literal types or as `<repr>` for
/// non literals
struct DisplayRepresentation<'db> {
    ty: Type<'db>,
    db: &'db dyn Db,
}

impl Display for DisplayRepresentation<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.ty {
            Type::Any => f.write_str("Any"),
            Type::Never => f.write_str("Never"),
            Type::Unknown => f.write_str("Unknown"),
            Type::Instance(InstanceType { class }) => {
                let representation = match class.known(self.db) {
                    Some(KnownClass::NoneType) => "None",
                    Some(KnownClass::NoDefaultType) => "NoDefault",
                    _ => class.name(self.db),
                };
                f.write_str(representation)
            }
            // `[Type::Todo]`'s display should be explicit that is not a valid display of
            // any other type
            Type::Todo(todo) => write!(f, "@Todo{todo}"),
            Type::ModuleLiteral(module) => {
                write!(f, "<module '{}'>", module.module(self.db).name())
            }
            // TODO functions and classes should display using a fully qualified name
            Type::ClassLiteral(ClassLiteralType { class }) => f.write_str(class.name(self.db)),
            Type::SubclassOf(SubclassOfType {
                base: ClassBase::Class(class),
            }) => {
                // Only show the bare class name here; ClassBase::display would render this as
                // type[<class 'Foo'>] instead of type[Foo].
                write!(f, "type[{}]", class.name(self.db))
            }
            Type::SubclassOf(SubclassOfType { base }) => {
                write!(f, "type[{}]", base.display(self.db))
            }
            Type::KnownInstance(known_instance) => f.write_str(known_instance.repr(self.db)),
            Type::FunctionLiteral(function) => f.write_str(function.name(self.db)),
            Type::Union(union) => union.display(self.db).fmt(f),
            Type::Intersection(intersection) => intersection.display(self.db).fmt(f),
            Type::IntLiteral(n) => n.fmt(f),
            Type::BooleanLiteral(boolean) => f.write_str(if boolean { "True" } else { "False" }),
            Type::StringLiteral(string) => string.display(self.db).fmt(f),
            Type::LiteralString => f.write_str("LiteralString"),
            Type::BytesLiteral(bytes) => {
                let escape =
                    AsciiEscape::with_preferred_quote(bytes.value(self.db).as_ref(), Quote::Double);

                escape.bytes_repr().write(f)
            }
            Type::SliceLiteral(slice) => {
                f.write_str("slice[")?;
                if let Some(start) = slice.start(self.db) {
                    write!(f, "Literal[{start}]")?;
                } else {
                    f.write_str("None")?;
                }

                f.write_str(", ")?;

                if let Some(stop) = slice.stop(self.db) {
                    write!(f, "Literal[{stop}]")?;
                } else {
                    f.write_str("None")?;
                }

                if let Some(step) = slice.step(self.db) {
                    write!(f, ", Literal[{step}]")?;
                }

                f.write_str("]")
            }
            Type::Tuple(tuple) => {
                f.write_str("tuple[")?;
                let elements = tuple.elements(self.db);
                if elements.is_empty() {
                    f.write_str("()")?;
                } else {
                    elements.display(self.db).fmt(f)?;
                }
                f.write_str("]")
            }
            Type::AlwaysTruthy => f.write_str("AlwaysTruthy"),
            Type::AlwaysFalsy => f.write_str("AlwaysFalsy"),
        }
    }
}

impl<'db> UnionType<'db> {
    fn display(&'db self, db: &'db dyn Db) -> DisplayUnionType<'db> {
        DisplayUnionType { db, ty: self }
    }
}

struct DisplayUnionType<'db> {
    ty: &'db UnionType<'db>,
    db: &'db dyn Db,
}

impl Display for DisplayUnionType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let elements = self.ty.elements(self.db);

        // Group condensed-display types by kind.
        let mut grouped_condensed_kinds = FxHashMap::default();

        for element in elements {
            if let Ok(kind) = CondensedDisplayTypeKind::try_from(*element) {
                grouped_condensed_kinds
                    .entry(kind)
                    .or_insert_with(Vec::new)
                    .push(*element);
            }
        }

        let mut join = f.join(" | ");

        for element in elements {
            if let Ok(kind) = CondensedDisplayTypeKind::try_from(*element) {
                let Some(mut condensed_kind) = grouped_condensed_kinds.remove(&kind) else {
                    continue;
                };
                if kind == CondensedDisplayTypeKind::Int {
                    condensed_kind.sort_unstable_by_key(|ty| ty.expect_int_literal());
                }
                join.entry(&DisplayLiteralGroup {
                    literals: condensed_kind,
                    db: self.db,
                });
            } else {
                join.entry(&element.display(self.db));
            }
        }

        join.finish()?;

        debug_assert!(grouped_condensed_kinds.is_empty());

        Ok(())
    }
}

impl fmt::Debug for DisplayUnionType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)
    }
}

struct DisplayLiteralGroup<'db> {
    literals: Vec<Type<'db>>,
    db: &'db dyn Db,
}

impl Display for DisplayLiteralGroup<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("Literal[")?;
        f.join(", ")
            .entries(self.literals.iter().map(|ty| ty.representation(self.db)))
            .finish()?;
        f.write_str("]")
    }
}

/// Enumeration of literal types that are displayed in a "condensed way" inside `Literal` slices.
///
/// For example, `Literal[1] | Literal[2]` is displayed as `"Literal[1, 2]"`.
/// Not all `Literal` types are displayed using `Literal` slices
/// (e.g. it would be inappropriate to display `LiteralString`
/// as `Literal[LiteralString]`).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
enum CondensedDisplayTypeKind {
    Class,
    Function,
    Int,
    String,
    Bytes,
}

impl TryFrom<Type<'_>> for CondensedDisplayTypeKind {
    type Error = ();

    fn try_from(value: Type<'_>) -> Result<Self, Self::Error> {
        match value {
            Type::ClassLiteral(_) => Ok(Self::Class),
            Type::FunctionLiteral(_) => Ok(Self::Function),
            Type::IntLiteral(_) => Ok(Self::Int),
            Type::StringLiteral(_) => Ok(Self::String),
            Type::BytesLiteral(_) => Ok(Self::Bytes),
            _ => Err(()),
        }
    }
}

impl<'db> IntersectionType<'db> {
    fn display(&'db self, db: &'db dyn Db) -> DisplayIntersectionType<'db> {
        DisplayIntersectionType { db, ty: self }
    }
}

struct DisplayIntersectionType<'db> {
    ty: &'db IntersectionType<'db>,
    db: &'db dyn Db,
}

impl Display for DisplayIntersectionType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let tys = self
            .ty
            .positive(self.db)
            .iter()
            .map(|&ty| DisplayMaybeNegatedType {
                ty,
                db: self.db,
                negated: false,
            })
            .chain(
                self.ty
                    .negative(self.db)
                    .iter()
                    .map(|&ty| DisplayMaybeNegatedType {
                        ty,
                        db: self.db,
                        negated: true,
                    }),
            );
        f.join(" & ").entries(tys).finish()
    }
}

impl fmt::Debug for DisplayIntersectionType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)
    }
}

struct DisplayMaybeNegatedType<'db> {
    ty: Type<'db>,
    db: &'db dyn Db,
    negated: bool,
}

impl Display for DisplayMaybeNegatedType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.negated {
            f.write_str("~")?;
        }
        self.ty.display(self.db).fmt(f)
    }
}

pub(crate) trait TypeArrayDisplay<'db> {
    fn display(&self, db: &'db dyn Db) -> DisplayTypeArray;
}

impl<'db> TypeArrayDisplay<'db> for Box<[Type<'db>]> {
    fn display(&self, db: &'db dyn Db) -> DisplayTypeArray {
        DisplayTypeArray { types: self, db }
    }
}

impl<'db> TypeArrayDisplay<'db> for Vec<Type<'db>> {
    fn display(&self, db: &'db dyn Db) -> DisplayTypeArray {
        DisplayTypeArray { types: self, db }
    }
}

pub(crate) struct DisplayTypeArray<'b, 'db> {
    types: &'b [Type<'db>],
    db: &'db dyn Db,
}

impl Display for DisplayTypeArray<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.join(", ")
            .entries(self.types.iter().map(|ty| ty.display(self.db)))
            .finish()
    }
}

impl<'db> StringLiteralType<'db> {
    fn display(&'db self, db: &'db dyn Db) -> DisplayStringLiteralType<'db> {
        DisplayStringLiteralType { db, ty: self }
    }
}

struct DisplayStringLiteralType<'db> {
    ty: &'db StringLiteralType<'db>,
    db: &'db dyn Db,
}

impl Display for DisplayStringLiteralType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let value = self.ty.value(self.db);
        f.write_char('"')?;
        for ch in value.chars() {
            match ch {
                // `escape_debug` will escape even single quotes, which is not necessary for our
                // use case as we are already using double quotes to wrap the string.
                '\'' => f.write_char('\'')?,
                _ => write!(f, "{}", ch.escape_debug())?,
            }
        }
        f.write_char('"')
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithTestSystem;

    use crate::db::tests::setup_db;
    use crate::types::{global_symbol, SliceLiteralType, StringLiteralType, Type, UnionType};

    #[test]
    fn test_condense_literal_display_by_type() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/main.py",
            "
            def foo(x: int) -> int:
                return x + 1

            def bar(s: str) -> str:
                return s

            class A: ...
            class B: ...
            ",
        )?;
        let mod_file = system_path_to_file(&db, "src/main.py").expect("file to exist");

        let union_elements = &[
            Type::Unknown,
            Type::IntLiteral(-1),
            global_symbol(&db, mod_file, "A").expect_type(),
            Type::string_literal(&db, "A"),
            Type::bytes_literal(&db, &[0u8]),
            Type::bytes_literal(&db, &[7u8]),
            Type::IntLiteral(0),
            Type::IntLiteral(1),
            Type::string_literal(&db, "B"),
            global_symbol(&db, mod_file, "foo").expect_type(),
            global_symbol(&db, mod_file, "bar").expect_type(),
            global_symbol(&db, mod_file, "B").expect_type(),
            Type::BooleanLiteral(true),
            Type::none(&db),
        ];
        let union = UnionType::from_elements(&db, union_elements).expect_union();
        let display = format!("{}", union.display(&db));
        assert_eq!(
            display,
            concat!(
                "Unknown | ",
                "Literal[-1, 0, 1] | ",
                "Literal[A, B] | ",
                "Literal[\"A\", \"B\"] | ",
                "Literal[b\"\\x00\", b\"\\x07\"] | ",
                "Literal[foo, bar] | ",
                "Literal[True] | ",
                "None"
            )
        );
        Ok(())
    }

    #[test]
    fn test_slice_literal_display() {
        let db = setup_db();

        assert_eq!(
            Type::SliceLiteral(SliceLiteralType::new(&db, None, None, None))
                .display(&db)
                .to_string(),
            "slice[None, None]"
        );
        assert_eq!(
            Type::SliceLiteral(SliceLiteralType::new(&db, Some(1), None, None))
                .display(&db)
                .to_string(),
            "slice[Literal[1], None]"
        );
        assert_eq!(
            Type::SliceLiteral(SliceLiteralType::new(&db, None, Some(2), None))
                .display(&db)
                .to_string(),
            "slice[None, Literal[2]]"
        );
        assert_eq!(
            Type::SliceLiteral(SliceLiteralType::new(&db, Some(1), Some(5), None))
                .display(&db)
                .to_string(),
            "slice[Literal[1], Literal[5]]"
        );
        assert_eq!(
            Type::SliceLiteral(SliceLiteralType::new(&db, Some(1), Some(5), Some(2)))
                .display(&db)
                .to_string(),
            "slice[Literal[1], Literal[5], Literal[2]]"
        );
        assert_eq!(
            Type::SliceLiteral(SliceLiteralType::new(&db, None, None, Some(2)))
                .display(&db)
                .to_string(),
            "slice[None, None, Literal[2]]"
        );
    }

    #[test]
    fn string_literal_display() {
        let db = setup_db();

        assert_eq!(
            Type::StringLiteral(StringLiteralType::new(&db, r"\n"))
                .display(&db)
                .to_string(),
            r#"Literal["\\n"]"#
        );
        assert_eq!(
            Type::StringLiteral(StringLiteralType::new(&db, "'"))
                .display(&db)
                .to_string(),
            r#"Literal["'"]"#
        );
        assert_eq!(
            Type::StringLiteral(StringLiteralType::new(&db, r#"""#))
                .display(&db)
                .to_string(),
            r#"Literal["\""]"#
        );
    }
}
