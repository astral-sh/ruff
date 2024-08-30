//! Display implementations for types.

use std::fmt::{Display, Formatter};

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

impl Display for DisplayType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
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
            Type::Class(class) => write!(f, "Literal[{}]", class.name(self.db)),
            Type::Instance(class) => f.write_str(class.name(self.db)),
            Type::Function(function) => write!(f, "Literal[{}]", function.name(self.db)),
            Type::Union(union) => union.display(self.db).fmt(f),
            Type::Intersection(intersection) => intersection.display(self.db).fmt(f),
            Type::IntLiteral(n) => write!(f, "Literal[{n}]"),
            Type::BooleanLiteral(boolean) => {
                write!(f, "Literal[{}]", if *boolean { "True" } else { "False" })
            }
            Type::StringLiteral(string) => write!(
                f,
                r#"Literal["{}"]"#,
                string.value(self.db).replace('"', r#"\""#)
            ),
            Type::LiteralString => write!(f, "LiteralString"),
            Type::BytesLiteral(bytes) => {
                let escape =
                    AsciiEscape::with_preferred_quote(bytes.value(self.db).as_ref(), Quote::Double);

                f.write_str("Literal[")?;
                escape.bytes_repr().write(f)?;
                f.write_str("]")
            }
        }
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

        // Group literals by type: {int, string, bytes, boolean, function, class}
        let mut all_literals: Vec<Vec<String>> =
            vec![vec![], vec![], vec![], vec![], vec![], vec![]];
        let mut other_types = vec![];

        union
            .elements(self.db)
            .iter()
            .copied()
            .for_each(|ty| match ty {
                // Write a String for each literal type appended in order of appearance
                Type::IntLiteral(n) => all_literals[0].push(format!("{n}")),
                Type::StringLiteral(s) => all_literals[1].push(format!("\"{}\"", s.value(self.db))),
                Type::BytesLiteral(b) => {
                    let escape =
                        AsciiEscape::with_preferred_quote(b.value(self.db).as_ref(), Quote::Double);
                    if let Some(string) = escape.bytes_repr().to_string() {
                        all_literals[2].push(string);
                    }
                }
                Type::BooleanLiteral(b) => all_literals[3].push(if b {
                    "True".to_string()
                } else {
                    "False".to_string()
                }),
                Type::Class(c) => all_literals[4].push(c.name(self.db).to_string()),
                Type::Function(f) => all_literals[5].push(f.name(self.db).to_string()),
                _ => other_types.push(ty),
            });

        all_literals
            .iter_mut()
            .for_each(|group| group.sort_unstable());

        let mut first = true;
        for literals_group in all_literals.iter().filter(|group| !group.is_empty()) {
            if !first {
                f.write_str(" | ")?;
            };
            first = false;

            f.write_str("Literal[")?;
            for (i, literal) in literals_group.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "{literal}")?;
            }
            f.write_str("]")?;
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
            // Bytes Literals
            Type::BytesLiteral(BytesLiteralType::new(&db, Box::from([7]))),
            Type::BytesLiteral(BytesLiteralType::new(&db, Box::from([0]))),
            // Strings Literals
            Type::StringLiteral(StringLiteralType::new(&db, Box::from("B"))),
            Type::StringLiteral(StringLiteralType::new(&db, Box::from("A"))),
            // Integers Literals
            Type::IntLiteral(1),
            Type::IntLiteral(-1),
            Type::IntLiteral(0),
            // Other non Literals
            global_symbol_ty_by_name(&db, mod_file, "foo"),
            global_symbol_ty_by_name(&db, mod_file, "bar"),
            global_symbol_ty_by_name(&db, mod_file, "A"),
            global_symbol_ty_by_name(&db, mod_file, "B"),
            Type::None,
            // Booleans Literals
            Type::BooleanLiteral(false),
            Type::BooleanLiteral(true),
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
                "Literal[\"A\", \"B\"] | ",
                "Literal[b\"\\x00\", b\"\\x07\"] | ",
                "Literal[False, True] | ",
                "Literal[A, B] | ",
                "Literal[bar, foo] | ",
                "None"
            )
        );
        Ok(())
    }
}
