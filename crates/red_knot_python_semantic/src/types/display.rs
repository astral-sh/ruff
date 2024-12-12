//! Display implementations for types.

use ruff_python_ast::str::Quote;
use ruff_python_literal::escape::AsciiEscape;
use std::fmt::{self, Arguments, Formatter, Write};

use crate::types::mro::ClassBase;
use crate::types::{
    ClassLiteralType, InstanceType, IntersectionType, KnownClass, StringLiteralType,
    SubclassOfType, Type, UnionType,
};
use crate::Db;
use rustc_hash::FxHashMap;

impl<'db> Type<'db> {
    fn representation(self) -> Representation<'db> {
        Representation { ty: self }
    }

    pub fn display(self, db: &'db dyn Db) -> DisplayWrapper<'db, Type<'db>> {
        DisplayWrapper::new(db, self)
    }

    pub fn display_slice<'types>(
        db: &'db dyn Db,
        types: &'types [Type<'db>],
    ) -> DisplayWrapper<'db, &'types [Type<'db>]> {
        DisplayWrapper::new(db, types)
    }
}

impl<'db> DisplayType<'db> for Type<'db> {
    fn fmt(&self, f: &mut TypeFormatter<'db, '_>) -> fmt::Result {
        if f.visited.contains(self) {
            return f.write_str("<recursion>");
        }
        f.visited.push(*self);

        let representation = self.representation();

        if matches!(
            self,
            Type::IntLiteral(_)
                | Type::BooleanLiteral(_)
                | Type::StringLiteral(_)
                | Type::BytesLiteral(_)
                | Type::ClassLiteral(_)
                | Type::FunctionLiteral(_)
        ) {
            f.write_str("Literal[")?;
            representation.fmt(f)?;
            f.write_str("]")?;
        } else {
            representation.fmt(f)?;
        }

        let removed = f.visited.pop();
        debug_assert_eq!(removed, Some(*self));

        Ok(())
    }
}

/// Writes the string representation of a type, which is the value displayed either as
/// `Literal[<repr>]` or `Literal[<repr1>, <repr2>]` for literal types or as `<repr>` for
/// non literals
struct Representation<'db> {
    ty: Type<'db>,
}

impl<'db> DisplayType<'db> for Representation<'db> {
    fn fmt(&self, f: &mut TypeFormatter<'db, '_>) -> fmt::Result {
        match self.ty {
            Type::Any => f.write_str("Any"),
            Type::Never => f.write_str("Never"),
            Type::Unknown => f.write_str("Unknown"),
            Type::Instance(InstanceType { class }) => {
                let representation = match class.known(f.db()) {
                    Some(KnownClass::NoneType) => "None",
                    Some(KnownClass::NoDefaultType) => "NoDefault",
                    _ => class.name(f.db()),
                };
                f.write_str(representation)
            }
            // `[Type::Todo]`'s display should be explicit that is not a valid display of
            // any other type
            Type::Todo(todo) => write!(f, "@Todo{todo}"),
            Type::ModuleLiteral(file) => {
                write!(f, "<module '{:?}'>", file.path(f.db()))
            }
            // TODO functions and classes should display using a fully qualified name
            Type::ClassLiteral(ClassLiteralType { class }) => f.write_str(class.name(f.db())),
            Type::SubclassOf(SubclassOfType {
                base: ClassBase::Class(class),
            }) => {
                // Only show the bare class name here; ClassBase::display would render this as
                // type[<class 'Foo'>] instead of type[Foo].
                write!(f, "type[{}]", class.name(f.db()))
            }
            Type::SubclassOf(SubclassOfType { base }) => {
                write!(f, "type[{}]", base.display(f.db()))
            }
            Type::KnownInstance(known_instance) => f.write_str(known_instance.repr(f.db())),
            Type::FunctionLiteral(function) => f.write_str(function.name(f.db())),
            Type::Union(union) => union.fmt(f),
            Type::Intersection(intersection) => intersection.fmt(f),
            Type::IntLiteral(n) => write!(f, "{n}"),
            Type::BooleanLiteral(boolean) => f.write_str(if boolean { "True" } else { "False" }),
            Type::StringLiteral(string) => string.fmt(f),
            Type::LiteralString => f.write_str("LiteralString"),
            Type::BytesLiteral(bytes) => {
                let escape =
                    AsciiEscape::with_preferred_quote(bytes.value(f.db()).as_ref(), Quote::Double);

                escape.bytes_repr().write(f)
            }
            Type::SliceLiteral(slice) => {
                f.write_str("slice[")?;
                if let Some(start) = slice.start(f.db()) {
                    write!(f, "Literal[{start}]")?;
                } else {
                    f.write_str("None")?;
                }

                f.write_str(", ")?;

                if let Some(stop) = slice.stop(f.db()) {
                    write!(f, "Literal[{stop}]")?;
                } else {
                    f.write_str("None")?;
                }

                if let Some(step) = slice.step(f.db()) {
                    write!(f, ", Literal[{step}]")?;
                }

                f.write_str("]")
            }
            Type::Tuple(tuple) => {
                f.write_str("tuple[")?;
                let elements = tuple.elements(f.db());
                if elements.is_empty() {
                    f.write_str("()")?;
                } else {
                    elements.fmt(f)?;
                }
                f.write_str("]")
            }
        }
    }
}

impl<'db> DisplayType<'db> for UnionType<'db> {
    fn fmt(&self, f: &mut TypeFormatter<'db, '_>) -> fmt::Result {
        let elements = self.elements(f.db());

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
                join.entry(&LiteralGroup {
                    literals: condensed_kind,
                });
            } else {
                join.entry(element);
            }
        }

        join.finish()?;

        debug_assert!(grouped_condensed_kinds.is_empty());

        Ok(())
    }
}

struct LiteralGroup<'db> {
    literals: Vec<Type<'db>>,
}

impl<'db> DisplayType<'db> for LiteralGroup<'db> {
    fn fmt(&self, f: &mut TypeFormatter<'db, '_>) -> fmt::Result {
        f.write_str("Literal[")?;
        f.join(", ")
            .entries(self.literals.iter().map(|ty| ty.representation()))
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

impl<'db> DisplayType<'db> for IntersectionType<'db> {
    fn fmt(&self, f: &mut TypeFormatter<'db, '_>) -> fmt::Result {
        let tys = self
            .positive(f.db())
            .iter()
            .map(|&ty| MaybeNegatedType { ty, negated: false })
            .chain(
                self.negative(f.db())
                    .iter()
                    .map(|&ty| MaybeNegatedType { ty, negated: true }),
            );
        f.join(" & ").entries(tys).finish()
    }
}

struct MaybeNegatedType<'db> {
    ty: Type<'db>,
    negated: bool,
}

impl<'db> DisplayType<'db> for MaybeNegatedType<'db> {
    fn fmt(&self, f: &mut TypeFormatter<'db, '_>) -> fmt::Result {
        if self.negated {
            f.write_str("~")?;
        }

        self.ty.fmt(f)
    }
}

impl<'db> DisplayType<'db> for [Type<'db>] {
    fn fmt(&self, f: &mut TypeFormatter<'db, '_>) -> fmt::Result {
        f.join(", ").entries(self.iter().copied()).finish()
    }
}

impl<'db> DisplayType<'db> for &[Type<'db>] {
    fn fmt(&self, f: &mut TypeFormatter<'db, '_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<'db> DisplayType<'db> for Box<[Type<'db>]> {
    fn fmt(&self, f: &mut TypeFormatter<'db, '_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<'db> DisplayType<'db> for Vec<Type<'db>> {
    fn fmt(&self, f: &mut TypeFormatter<'db, '_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<'db> DisplayType<'db> for StringLiteralType<'db> {
    fn fmt(&self, f: &mut TypeFormatter<'db, '_>) -> fmt::Result {
        let value = self.value(f.db());
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

struct TypeFormatter<'db, 'write> {
    db: &'db dyn Db,
    write: &'write mut dyn Write,
    visited: Vec<Type<'db>>,
}

impl<'db, 'write> TypeFormatter<'db, 'write> {
    pub(crate) fn new(db: &'db dyn Db, write: &'write mut dyn Write) -> Self {
        Self {
            db,
            write,
            visited: Vec::default(),
        }
    }

    pub(crate) fn join<'f>(&'f mut self, separator: &'static str) -> Join<'db, 'f, 'write> {
        Join {
            fmt: self,
            separator,
            result: Ok(()),
            seen_first: false,
        }
    }

    pub(crate) fn db(&self) -> &'db dyn Db {
        self.db
    }
}

impl Write for TypeFormatter<'_, '_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write.write_str(s)
    }

    fn write_char(&mut self, c: char) -> fmt::Result {
        self.write.write_char(c)
    }

    fn write_fmt(&mut self, args: Arguments<'_>) -> fmt::Result {
        self.write.write_fmt(args)
    }
}

trait DisplayType<'db> {
    fn fmt(&self, f: &mut TypeFormatter<'db, '_>) -> fmt::Result;
}

pub struct DisplayWrapper<'db, T> {
    db: &'db dyn Db,
    inner: T,
}

impl<'db, T> DisplayWrapper<'db, T> {
    fn new(db: &'db dyn Db, inner: T) -> Self {
        Self { db, inner }
    }
}

impl<'db, T> DisplayType<'db> for DisplayWrapper<'db, T>
where
    T: DisplayType<'db>,
{
    fn fmt(&self, f: &mut TypeFormatter<'db, '_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<'db, T> fmt::Display for DisplayWrapper<'db, T>
where
    T: DisplayType<'db>,
{
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut f = TypeFormatter::new(self.db, f);
        DisplayType::fmt(self, &mut f)
    }
}

struct Join<'db, 'f, 'write> {
    fmt: &'f mut TypeFormatter<'db, 'write>,
    separator: &'static str,
    result: fmt::Result,
    seen_first: bool,
}

impl<'db> Join<'db, '_, '_> {
    fn entry(&mut self, item: &dyn DisplayType<'db>) -> &mut Self {
        if self.seen_first {
            self.result = self
                .result
                .and_then(|()| self.fmt.write_str(self.separator));
        } else {
            self.seen_first = true;
        }
        self.result = self.result.and_then(|()| item.fmt(self.fmt));
        self
    }

    fn entries<I, F>(&mut self, items: I) -> &mut Self
    where
        I: IntoIterator<Item = F>,
        F: DisplayType<'db>,
    {
        for item in items {
            self.entry(&item);
        }
        self
    }

    fn finish(&mut self) -> fmt::Result {
        self.result
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
        let display = format!("{}", Type::Union(union).display(&db));
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
