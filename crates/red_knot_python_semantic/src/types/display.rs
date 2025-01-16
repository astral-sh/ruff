//! Display implementations for types.

use std::fmt::{self, Display, Formatter, Write};

use ruff_db::display::FormatterJoinExtension;
use ruff_python_ast::str::Quote;
use ruff_python_literal::escape::AsciiEscape;

use crate::types::class_base::ClassBase;
use crate::types::{
    ClassLiteralType, InstanceType, IntersectionType, KnownClass, StringLiteralType, Type,
    UnionType,
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
            Type::Dynamic(dynamic) => dynamic.fmt(f),
            Type::Never => f.write_str("Never"),
            Type::Instance(InstanceType { class }) => {
                let representation = match class.known(self.db) {
                    Some(KnownClass::NoneType) => "None",
                    Some(KnownClass::NoDefaultType) => "NoDefault",
                    _ => class.name(self.db),
                };
                f.write_str(representation)
            }
            Type::ModuleLiteral(module) => {
                write!(f, "<module '{}'>", module.module(self.db).name())
            }
            // TODO functions and classes should display using a fully qualified name
            Type::ClassLiteral(ClassLiteralType { class }) => f.write_str(class.name(self.db)),
            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of() {
                // Only show the bare class name here; ClassBase::display would render this as
                // type[<class 'Foo'>] instead of type[Foo].
                ClassBase::Class(class) => write!(f, "type[{}]", class.name(self.db)),
                ClassBase::Dynamic(dynamic) => write!(f, "type[{dynamic}]"),
            },
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
                let Some(condensed_kind) = grouped_condensed_kinds.remove(&kind) else {
                    continue;
                };
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
/// For example, `Literal[1] | Literal[2] | Literal["s"]` is displayed as `"Literal[1, 2, "s"]"`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
enum CondensedDisplayTypeKind {
    Class,
    Function,
    LiteralExpression,
}

impl TryFrom<Type<'_>> for CondensedDisplayTypeKind {
    type Error = ();

    fn try_from(value: Type<'_>) -> Result<Self, Self::Error> {
        match value {
            Type::ClassLiteral(_) => Ok(Self::Class),
            Type::FunctionLiteral(_) => Ok(Self::Function),
            Type::IntLiteral(_)
            | Type::StringLiteral(_)
            | Type::BytesLiteral(_)
            | Type::BooleanLiteral(_) => Ok(Self::LiteralExpression),
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

impl<'db> TypeArrayDisplay<'db> for [Type<'db>] {
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
    use crate::db::tests::setup_db;
    use crate::types::{SliceLiteralType, StringLiteralType, Type};

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
