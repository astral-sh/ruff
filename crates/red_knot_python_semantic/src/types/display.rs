//! Display implementations for types.

use std::fmt::{Display, Formatter};

use rustc_hash::FxHashMap;

use ruff_python_ast::str::Quote;
use ruff_python_literal::escape::AsciiEscape;

use crate::types::{IntersectionType, Type, UnionType};
use crate::Db;

impl<'db> Type<'db> {
    pub fn display(&'db self, db: &'db dyn Db) -> DisplayType<'db> {
        DisplayType { ty: self, db }
    }
}

#[derive(Copy, Clone)]
pub struct DisplayType<'db> {
    ty: &'db Type<'db>,
    db: &'db dyn Db,
}

impl DisplayType<'_> {
    /// Writes the string representation of a type, which is the value displayed either as
    /// `Literal[<repr>]` or `Literal[<repr1>, <repr2>]` for literal types or as `<repr>` for
    /// non literals
    fn write_representation(&self, f: &mut Formatter) -> std::fmt::Result {
        // This methods avoids duplicating individual types representation logic in
        // `UnionType`
        match self.ty {
            Type::Any => f.write_str("Any"),
            Type::Never => f.write_str("Never"),
            Type::Unknown => f.write_str("Unknown"),
            Type::Unbound => f.write_str("Unbound"),
            Type::None => f.write_str("None"),
            Type::Module(file) => {
                write!(f, "<module '{:?}'>", file.path(self.db))
            }
            // TODO functions and classes should display using a fully qualified name
            Type::Class(class) => f.write_str(class.name(self.db)),
            Type::Instance(class) => f.write_str(class.name(self.db)),
            Type::Function(function) => f.write_str(function.name(self.db)),
            Type::Union(union) => union.display(self.db).fmt(f),
            Type::Intersection(intersection) => intersection.display(self.db).fmt(f),
            Type::IntLiteral(n) => write!(f, "{n}"),
            Type::BooleanLiteral(boolean) => f.write_str(if *boolean { "True" } else { "False" }),
            Type::StringLiteral(string) => {
                write!(f, r#""{}""#, string.value(self.db).replace('"', r#"\""#))
            }
            Type::LiteralString => f.write_str("LiteralString"),
            Type::BytesLiteral(bytes) => {
                let escape =
                    AsciiEscape::with_preferred_quote(bytes.value(self.db).as_ref(), Quote::Double);

                escape.bytes_repr().write(f)
            }
        }
    }
}

impl Display for DisplayType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let is_literal = self.ty.is_literal();
        if is_literal {
            f.write_str("Literal[")?;
        }
        self.write_representation(f)?;
        if is_literal {
            f.write_str("]")?;
        }
        Ok(())
    }
}

impl std::fmt::Debug for DisplayType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
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
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let union = self.ty;

        // Group literals types by discriminant
        let mut literals_map = FxHashMap::default();
        // Non-literal types are all in order of appearance
        let mut other_types: Vec<Type> = vec![];
        // Remember all discriminants in order of appearance, duplicates are not a problem
        let mut literals_discriminants = vec![];

        union.elements(self.db).iter().copied().for_each(|ty| {
            if ty.is_literal() {
                // Store literals in a map by discriminant (same variant)
                let discriminant = std::mem::discriminant(&ty);
                literals_map
                    .entry(discriminant)
                    .or_insert_with(Vec::new)
                    .push(ty);
                literals_discriminants.push(discriminant);
            } else {
                // Store non-literals in a separate list
                other_types.push(ty);
            }
        });

        let mut first = true;
        for discriminant in literals_discriminants {
            // On first encounter of discriminant, write all literals of that type. The group will
            // be removed for further occurrences of the same discriminant
            if let Some(literals_ty) = literals_map.remove(&discriminant) {
                if !first {
                    f.write_str(" | ")?;
                };
                first = false;

                f.write_str("Literal[")?;
                for (i, literal_ty) in literals_ty.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    literal_ty.display(self.db).write_representation(f)?;
                }
                f.write_str("]")?;
            }
        }

        for ty in other_types {
            if !first {
                f.write_str(" | ")?;
            };
            first = false;
            write!(f, "{}", ty.display(self.db))?;
        }

        Ok(())
    }
}

impl std::fmt::Debug for DisplayUnionType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
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
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut first = true;
        for (neg, ty) in self
            .ty
            .positive(self.db)
            .iter()
            .map(|ty| (false, ty))
            .chain(self.ty.negative(self.db).iter().map(|ty| (true, ty)))
        {
            if !first {
                f.write_str(" & ")?;
            };
            first = false;
            if neg {
                f.write_str("~")?;
            };
            write!(f, "{}", ty.display(self.db))?;
        }
        Ok(())
    }
}

impl std::fmt::Debug for DisplayIntersectionType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use crate::db::tests::TestDb;
    use crate::types::{
        global_symbol_ty_by_name, BytesLiteralType, StringLiteralType, Type, UnionBuilder,
    };
    use crate::{Program, ProgramSettings, PythonVersion, SearchPathSettings};
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};

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
        let mod_file = system_path_to_file(&db, "src/main.py").expect("Expected file to exist.");

        let vec: Vec<Type<'_>> = vec![
            Type::Unknown,
            Type::IntLiteral(-1),
            global_symbol_ty_by_name(&db, mod_file, "A"),
            Type::StringLiteral(StringLiteralType::new(&db, Box::from("A"))),
            Type::BytesLiteral(BytesLiteralType::new(&db, Box::from([0]))),
            Type::BytesLiteral(BytesLiteralType::new(&db, Box::from([7]))),
            Type::IntLiteral(0),
            Type::IntLiteral(1),
            Type::StringLiteral(StringLiteralType::new(&db, Box::from("B"))),
            global_symbol_ty_by_name(&db, mod_file, "foo"),
            global_symbol_ty_by_name(&db, mod_file, "bar"),
            global_symbol_ty_by_name(&db, mod_file, "B"),
            Type::BooleanLiteral(true),
            Type::None,
        ];
        let builder = vec.iter().fold(UnionBuilder::new(&db), |builder, literal| {
            builder.add(*literal)
        });
        let Type::Union(union) = builder.build() else {
            panic!("expected a union");
        };
        let display = format!("{}", union.display(&db));
        assert_eq!(
            display,
            concat!(
                "Literal[-1, 0, 1] | ",
                "Literal[A, B] | ",
                "Literal[\"A\", \"B\"] | ",
                "Literal[b\"\\x00\", b\"\\x07\"] | ",
                "Literal[foo, bar] | ",
                // TODO: Non literals always come after Literals, which might be unwanted
                "Literal[True] | ",
                "Unknown | ",
                "None"
            )
        );
        Ok(())
    }
}
