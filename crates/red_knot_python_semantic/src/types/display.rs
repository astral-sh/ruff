//! Display implementations for types.

use std::fmt::{self, Display, Formatter};

use ruff_db::display::FormatterJoinExtension;
use ruff_python_ast::str::Quote;
use ruff_python_literal::escape::AsciiEscape;

use crate::types::{IntersectionType, Type, UnionType};
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
                | Type::Class(_)
                | Type::Function(_)
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
            Type::Unbound => f.write_str("Unbound"),
            Type::None => f.write_str("None"),
            // `[Type::Todo]`'s display should be explicit that is not a valid display of
            // any other type
            Type::Todo => f.write_str("@Todo"),
            Type::Module(file) => {
                write!(f, "<module '{:?}'>", file.path(self.db))
            }
            // TODO functions and classes should display using a fully qualified name
            Type::Class(class) => f.write_str(class.name(self.db)),
            Type::Instance(class) => f.write_str(class.name(self.db)),
            Type::Function(function) => f.write_str(function.name(self.db)),
            Type::Union(union) => union.display(self.db).fmt(f),
            Type::Intersection(intersection) => intersection.display(self.db).fmt(f),
            Type::IntLiteral(n) => n.fmt(f),
            Type::BooleanLiteral(boolean) => f.write_str(if boolean { "True" } else { "False" }),
            Type::StringLiteral(string) => {
                write!(f, r#""{}""#, string.value(self.db).replace('"', r#"\""#))
            }
            Type::LiteralString => f.write_str("LiteralString"),
            Type::BytesLiteral(bytes) => {
                let escape =
                    AsciiEscape::with_preferred_quote(bytes.value(self.db).as_ref(), Quote::Double);

                escape.bytes_repr().write(f)
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

        // Group literal types by kind.
        let mut grouped_literals = FxHashMap::default();

        for element in elements {
            if let Ok(literal_kind) = LiteralTypeKind::try_from(*element) {
                grouped_literals
                    .entry(literal_kind)
                    .or_insert_with(Vec::new)
                    .push(*element);
            }
        }

        let mut join = f.join(" | ");

        for element in elements {
            if let Ok(literal_kind) = LiteralTypeKind::try_from(*element) {
                let Some(mut literals) = grouped_literals.remove(&literal_kind) else {
                    continue;
                };
                if literal_kind == LiteralTypeKind::IntLiteral {
                    literals.sort_unstable_by_key(|ty| ty.expect_int_literal());
                }
                join.entry(&DisplayLiteralGroup {
                    literals,
                    db: self.db,
                });
            } else {
                join.entry(&element.display(self.db));
            }
        }

        join.finish()?;

        debug_assert!(grouped_literals.is_empty());

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

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
enum LiteralTypeKind {
    Class,
    Function,
    IntLiteral,
    StringLiteral,
    BytesLiteral,
}

impl TryFrom<Type<'_>> for LiteralTypeKind {
    type Error = ();

    fn try_from(value: Type<'_>) -> Result<Self, Self::Error> {
        match value {
            Type::Class(_) => Ok(Self::Class),
            Type::Function(_) => Ok(Self::Function),
            Type::IntLiteral(_) => Ok(Self::IntLiteral),
            Type::StringLiteral(_) => Ok(Self::StringLiteral),
            Type::BytesLiteral(_) => Ok(Self::BytesLiteral),
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

impl<'db> Display for DisplayMaybeNegatedType<'db> {
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

impl<'db> Display for DisplayTypeArray<'_, 'db> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.join(", ")
            .entries(self.types.iter().map(|ty| ty.display(self.db)))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};

    use crate::db::tests::TestDb;
    use crate::types::{global_symbol_ty, BytesLiteralType, StringLiteralType, Type, UnionType};
    use crate::{Program, ProgramSettings, PythonVersion, SearchPathSettings};

    fn setup_db() -> TestDb {
        let db = TestDb::new();

        let src_root = SystemPathBuf::from("/src");
        db.memory_file_system()
            .create_directory_all(&src_root)
            .unwrap();

        Program::from_settings(
            &db,
            &ProgramSettings {
                target_version: PythonVersion::default(),
                search_paths: SearchPathSettings::new(src_root),
            },
        )
        .expect("Valid search path settings");

        db
    }

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
            global_symbol_ty(&db, mod_file, "A"),
            Type::StringLiteral(StringLiteralType::new(&db, Box::from("A"))),
            Type::BytesLiteral(BytesLiteralType::new(&db, Box::from([0]))),
            Type::BytesLiteral(BytesLiteralType::new(&db, Box::from([7]))),
            Type::IntLiteral(0),
            Type::IntLiteral(1),
            Type::StringLiteral(StringLiteralType::new(&db, Box::from("B"))),
            global_symbol_ty(&db, mod_file, "foo"),
            global_symbol_ty(&db, mod_file, "bar"),
            global_symbol_ty(&db, mod_file, "B"),
            Type::BooleanLiteral(true),
            Type::None,
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
}
