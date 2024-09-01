//! Display implementations for types.

use std::fmt::{Display, Formatter};

use hashbrown::HashMap;

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
    /// Returns the string representation of a type, which is the value displayed either as
    /// `Literal[<repr>]` or `Literal[<repr1>, <repr2>]` for literal types or as `<repr>` for non
    /// literals
    fn representation(&self) -> String {
        // This methods avoids duplicating individual types representation logic in `UnionType`
        match self.ty {
            Type::Any => "Any".to_string(),
            Type::Never => "Never".to_string(),
            Type::Unknown => "Unknown".to_string(),
            Type::Unbound => "Unbound".to_string(),
            Type::None => "None".to_string(),
            Type::Module(file) => {
                format!("<module '{:?}'>", file.path(self.db))
            }
            // TODO functions and classes should display using a fully qualified name
            Type::Class(class) => class.name(self.db).to_string(),
            Type::Instance(class) => class.name(self.db).to_string(),
            Type::Function(function) => function.name(self.db).to_string(),
            Type::Union(union) => union.display(self.db).to_string(),
            Type::Intersection(intersection) => intersection.display(self.db).to_string(),
            Type::IntLiteral(n) => n.to_string(),
            Type::BooleanLiteral(boolean) => {
                if *boolean {
                    "True".to_string()
                } else {
                    "False".to_string()
                }
            }
            Type::StringLiteral(string) => {
                format!(r#""{}""#, string.value(self.db).replace('"', r#"\""#))
            }
            Type::LiteralString => "LiteralString".to_string(),
            Type::BytesLiteral(bytes) => {
                let escape =
                    AsciiEscape::with_preferred_quote(bytes.value(self.db).as_ref(), Quote::Double);
                escape.bytes_repr().to_string().unwrap_or_default()
            }
        }
    }
}

impl Display for DisplayType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.ty {
            // Literal types -> `Literal[<repr>]`
            Type::Class(_)
            | Type::Function(_)
            | Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::StringLiteral(_)
            | Type::BytesLiteral(_) => {
                f.write_str("Literal[")?;
                f.write_str(&self.representation())?;
                f.write_str("]")
            }
            // Non literal types -> `<repr>`
            _ => f.write_str(&self.representation()),
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

// Enumeration for literals that are displayed in a condensed form (e.g. `Literal[... | ...]`)
#[derive(Hash, PartialEq, Eq)]
enum CondensedLiteral {
    Int,
    String,
    Bytes,
    Function,
    Class,
}

impl CondensedLiteral {
    fn iter() -> impl Iterator<Item = &'static Self> {
        // Order of the literals in the condensed form
        [
            Self::Int,
            Self::String,
            Self::Bytes,
            Self::Class,
            Self::Function,
        ]
        .iter()
    }
}

impl Display for DisplayUnionType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let union = self.ty;

        // Group literals by type: {int, string, bytes, function, class}
        let mut literals_representation_map = HashMap::<CondensedLiteral, Vec<String>>::new();
        let mut other_types = vec![];

        union.elements(self.db).iter().copied().for_each(|ty| {
            // Find which group the type belongs to, None means other types
            let maybe_condensed_literal = match ty {
                Type::IntLiteral(_) => Some(CondensedLiteral::Int),
                Type::StringLiteral(_) => Some(CondensedLiteral::String),
                Type::BytesLiteral(_) => Some(CondensedLiteral::Bytes),
                Type::Class(_) => Some(CondensedLiteral::Class),
                Type::Function(_) => Some(CondensedLiteral::Function),
                _ => None,
            };
            if let Some(condensed_literal) = maybe_condensed_literal {
                // For literals, push the representation. Adding `Literal[...]` is done later
                literals_representation_map
                    .entry(condensed_literal)
                    .or_insert_with(Vec::new)
                    .push(ty.display(self.db).representation());
            } else {
                other_types.push(ty);
            }
        });

        literals_representation_map
            .values_mut()
            .for_each(|group| group.sort_unstable());

        let mut first = true;
        for condensed_literal in CondensedLiteral::iter() {
            if let Some(literals_representation) =
                literals_representation_map.remove(condensed_literal)
            {
                if !first {
                    f.write_str(" | ")?;
                };
                first = false;

                f.write_str("Literal[")?;
                for (i, literal_repr) in literals_representation.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{literal_repr}")?;
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
            // Functions
            global_symbol_ty_by_name(&db, mod_file, "foo"),
            global_symbol_ty_by_name(&db, mod_file, "bar"),
            // Classes
            global_symbol_ty_by_name(&db, mod_file, "A"),
            global_symbol_ty_by_name(&db, mod_file, "B"),
            // Other non-grouped types
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
                "Literal[\"A\", \"B\"] | ",
                "Literal[b\"\\x00\", b\"\\x07\"] | ",
                "Literal[A, B] | ",
                "Literal[bar, foo] | ",
                "Literal[True] | ",
                "None"
            )
        );
        Ok(())
    }
}
