use crate::Db;
use crate::goto::find_goto_target;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use ruff_text_size::{TextRange, TextSize};
use ty_python_semantic::SemanticModel;
use ty_python_semantic::TypeHierarchyClass;
use ty_python_semantic::types::Type;

/// Represents a type hierarchy item returned by the LSP type hierarchy requests.
#[derive(Debug, Clone)]
pub struct TypeHierarchyItem {
    /// The name of the type (e.g., `MyClass`).
    pub name: Name,
    /// The fully-qualified name or detail string (e.g., `mymodule.MyClass`).
    pub detail: Option<String>,
    /// The file containing the type definition.
    pub file: File,
    /// The range covering the full class definition.
    pub full_range: TextRange,
    /// The range of the class name (for selection/focus).
    pub selection_range: TextRange,
}

/// Prepare the type hierarchy at a given position.
///
/// Returns `None` if the position is not on a class definition or class reference.
pub fn prepare_type_hierarchy(
    db: &dyn Db,
    file: File,
    offset: TextSize,
) -> Option<TypeHierarchyItem> {
    let module = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    let goto_target = find_goto_target(&model, &module, offset)?;
    let ty = goto_target.inferred_type(&model)?;

    let hierarchy_class = ty_python_semantic::type_hierarchy_prepare(db, ty)?;
    Some(type_hierarchy_class_to_item(db, hierarchy_class))
}

/// Get the supertypes (base classes) of a type hierarchy item.
pub fn type_hierarchy_supertypes(
    db: &dyn Db,
    file: File,
    offset: TextSize,
) -> Vec<TypeHierarchyItem> {
    let Some(ty) = resolve_type_at(db, file, offset) else {
        return vec![];
    };
    ty_python_semantic::type_hierarchy_supertypes(db, ty)
        .into_iter()
        .map(|c| type_hierarchy_class_to_item(db, c))
        .collect()
}

/// Get the subtypes (derived classes) of a type hierarchy item.
pub fn type_hierarchy_subtypes(
    db: &dyn Db,
    file: File,
    offset: TextSize,
) -> Vec<TypeHierarchyItem> {
    let Some(ty) = resolve_type_at(db, file, offset) else {
        return vec![];
    };
    ty_python_semantic::type_hierarchy_subtypes(db, ty)
        .into_iter()
        .map(|c| type_hierarchy_class_to_item(db, c))
        .collect()
}

/// Returns the type of the symbol under the cursor at `offset` in `file`.
///
/// If a symbol could not be found at the given offset or its type could
/// not be inferred, `None` is returned.
fn resolve_type_at(db: &dyn Db, file: File, offset: TextSize) -> Option<Type<'_>> {
    let module = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);

    let goto_target = find_goto_target(&model, &module, offset)?;
    goto_target.inferred_type(&model)
}

fn type_hierarchy_class_to_item(db: &dyn Db, class: TypeHierarchyClass) -> TypeHierarchyItem {
    let detail = ty_module_resolver::file_to_module(db, class.file)
        .map(|module| module.name(db).to_string());

    TypeHierarchyItem {
        name: class.name,
        detail,
        file: class.file,
        full_range: class.full_range,
        selection_range: class.selection_range,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{CursorTest, cursor_test};

    #[test]
    fn prepare_type_hierarchy_on_class_def() {
        let test = cursor_test(
            r#"
            class MyC<CURSOR>lass:
                pass
            "#,
        );

        let item = test.prepare().unwrap();
        insta::assert_snapshot!(snapshot(&test.db, &[item]), @"/main.py:7:14 MyClass :: main");
    }

    #[test]
    fn prepare_type_hierarchy_on_class_usage() {
        let test = cursor_test(
            r#"
            class MyClass:
                pass

            x = MyC<CURSOR>lass()
            "#,
        );

        let item = test.prepare().unwrap();
        insta::assert_snapshot!(snapshot(&test.db, &[item]), @"/main.py:7:14 MyClass :: main");
    }

    #[test]
    fn prepare_type_hierarchy_on_non_class() {
        let test = cursor_test(
            r#"
            x = 4<CURSOR>2
            "#,
        );

        assert!(test.prepare().is_none());
    }

    #[test]
    fn supertypes_simple_inheritance() {
        let test = cursor_test(
            r#"
            class Base:
                pass

            class Der<CURSOR>ived(Base):
                pass
            "#,
        );

        let supertypes = test.supertypes();
        insta::assert_snapshot!(snapshot(&test.db, &supertypes), @"/main.py:7:11 Base :: main");
    }

    #[test]
    fn supertypes_multiple_inheritance() {
        let test = cursor_test(
            r#"
            class A:
                pass

            class B:
                pass

            class C<CURSOR>(A, B):
                pass
            "#,
        );

        let mut supertypes = test.supertypes();
        supertypes.sort_by(|a, b| a.name.cmp(&b.name));
        insta::assert_snapshot!(snapshot(&test.db, &supertypes), @r"
        /main.py:7:8 A :: main
        /main.py:26:27 B :: main
        ");
    }

    #[test]
    fn supertypes_generic_base() {
        let test = cursor_test(
            r#"
            from typing import Generic, TypeVar

            T = TypeVar("T")

            class Base(Generic[T]):
                pass

            class Der<CURSOR>ived(Base[int]):
                pass
            "#,
        );

        let supertypes = test.supertypes();
        insta::assert_snapshot!(snapshot(&test.db, &supertypes), @"/main.py:62:66 Base :: main");
    }

    #[test]
    fn supertypes_implicit_object() {
        let test = cursor_test(
            r#"
            class My<CURSOR>Class:
                pass
            "#,
        );

        let supertypes = test.supertypes();
        insta::assert_snapshot!(
            snapshot(&test.db, &supertypes),
            @"vendored://stdlib/builtins.pyi:3608:3614 object :: builtins",
        );
    }

    #[test]
    fn subtypes_simple() {
        let test = cursor_test(
            r#"
            class Ba<CURSOR>se:
                pass

            class Derived1(Base):
                pass

            class Derived2(Base):
                pass
            "#,
        );

        let mut subtypes = test.subtypes();
        subtypes.sort_by(|a, b| a.name.cmp(&b.name));
        insta::assert_snapshot!(snapshot(&test.db, &subtypes), @r"
        /main.py:29:37 Derived1 :: main
        /main.py:61:69 Derived2 :: main
        ");
    }

    #[test]
    fn subtypes_of_object_includes_implicit() {
        let test = cursor_test(
            r#"
            x: type[obje<CURSOR>ct]

            class ImplicitChild:
                pass

            class ExplicitChild(object):
                pass
            "#,
        );

        // `object` has hundreds of subtypes across typeshed,
        // so we check for specific items rather than snapshotting.
        let subtypes = test.subtypes();
        let names: Vec<_> = subtypes.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"ImplicitChild"));
        assert!(names.contains(&"ExplicitChild"));
    }

    #[test]
    fn subtypes_version_conditional() {
        let test = cursor_test(
            r#"
            import sys

            class Ba<CURSOR>se:
                pass

            if sys.version_info >= (3, 5):
                class ChildOld(Base):
                    pass

            if sys.version_info >= (3, 999):
                class ChildFuture(Base):
                    pass
            "#,
        );

        // As of 2026-02-25, the default Python version is 3.14, so
        // `ChildOld` should be a subtype of `Base`, but `ChildFuture`
        // should not.
        let subtypes = test.subtypes();
        insta::assert_snapshot!(snapshot(&test.db, &subtypes), @"/main.py:76:84 ChildOld :: main");
    }

    /// This is a regression test for a case where we would emit
    /// duplicate `MyEventType` subtypes of `str` because of the
    /// conditional definition based on Python version. Moreover, since
    /// our test's default Python version is newer than 3.11 and since
    /// we're asking for the *direct* subtypes of `str`, it follows
    /// that we shouldn't see `MyEventType` at all. That's because in
    /// the 3.11+ case, it inherit from `StrEnum` and not from `str`
    /// directly.
    ///
    /// So this test actually covers two different bugs: one for
    /// duplicates and another for not properly evaluating reachability
    /// constraints.
    #[test]
    fn subtypes_str_conditional_no_duplicates() {
        let test = cursor_test(
            r#"
            import sys

            if sys.version_info >= (3, 11):
                from enum import StrEnum
            else:
                from enum import Enum

            if sys.version_info >= (3, 11):
                class MyEventType(StrEnum):
                    Activate = "36"
                    ButtonPress = "4"
            else:
                class MyEventType(str, Enum):
                    Activate = "36"
                    ButtonPress = "4"

            x: st<CURSOR>r = "foo"
            "#,
        );

        let subtypes = test.subtypes();
        insta::assert_snapshot!(snapshot(&test.db, &subtypes), @r"
        vendored://stdlib/email/headerregistry.pyi:703:713 BaseHeader :: email.headerregistry
        vendored://stdlib/enum.pyi:18342:18349 StrEnum :: enum
        vendored://stdlib/pdb.pyi:38460:38465 _rstr :: pdb
        vendored://stdlib/xxlimited.pyi:113:116 Str :: xxlimited
        ");
    }

    /// Like `subtypes_str_conditional_no_duplicates`, but we make
    /// both branches inherit directly from `str`. We should get back
    /// `MyEventTypeA` and not `MyEventTypeB`.
    #[test]
    fn subtypes_str_conditional_one_direct_subtype() {
        let test = cursor_test(
            r#"
            import sys
            from enum import Enum

            if sys.version_info >= (3, 11):
                class MyEventTypeA(str, Enum):
                    Activate = "36"
                    ButtonPress = "4"
            else:
                class MyEventTypeB(str, Enum):
                    Activate = "36"
                    ButtonPress = "4"

            x: st<CURSOR>r = "foo"
            "#,
        );

        let subtypes = test.subtypes();
        insta::assert_snapshot!(snapshot(&test.db, &subtypes), @r"
        vendored://stdlib/email/headerregistry.pyi:703:713 BaseHeader :: email.headerregistry
        vendored://stdlib/enum.pyi:18342:18349 StrEnum :: enum
        /main.py:77:89 MyEventTypeA :: main
        vendored://stdlib/pdb.pyi:38460:38465 _rstr :: pdb
        vendored://stdlib/xxlimited.pyi:113:116 Str :: xxlimited
        ");
    }

    /// Dynamic classes created via `type()` can be prepared for the
    /// type hierarchy. The selection range highlights the variable
    /// name, not the `type()` call.
    #[test]
    fn dynamic_class_prepare_and_supertypes_variable_definition() {
        let test = cursor_test(
            r#"
            class Base:
                pass

            Dyn<CURSOR>amic = type("Dynamic", (Base,), {})
            "#,
        );

        let item = test.prepare().unwrap();
        insta::assert_snapshot!(snapshot(&test.db, &[item]), @"/main.py:23:30 Dynamic :: main");

        let supertypes = test.supertypes();
        insta::assert_snapshot!(snapshot(&test.db, &supertypes), @"/main.py:7:11 Base :: main");
    }

    /// Like `dynamic_class_prepare_and_supertypes_variable_definition`, but
    /// uses an inline `type` call and demonstrates the limitation in the
    /// current implementation (as of 2026-02-25).
    #[test]
    fn dynamic_class_prepare_and_supertypes_inline() {
        // This is "fine," but the offsets returned
        // for `Dynamic` as a supertype will result
        // in subsequent requests showing the type
        // hierarchy for `type` instead of `Dynamic`.
        let test = cursor_test(
            r#"
            class Base:
                pass

            class Su<CURSOR>per(type("Dynamic", (Base,), {})): pass
            "#,
        );
        let item = test.prepare().unwrap();
        insta::assert_snapshot!(snapshot(&test.db, &[item]), @"/main.py:29:34 Super :: main");
        let supertypes = test.supertypes();
        insta::assert_snapshot!(snapshot(&test.db, &supertypes), @"/main.py:35:63 Dynamic :: main");

        // We emulate that subsequent request here. This is a
        // limitation of our current type hierarchy implementation. I
        // think ideally we'd recognize the `type(...)` idiom and "see
        // through" it. But what if the user actually wants the type
        // hierarchy for `type`? Maybe we should only recognize the
        // idiom when the cursor is on the `"Dynamic"` string literal.
        // ---AG
        let test = cursor_test(
            r#"
            class Base:
                pass

            class Super(ty<CURSOR>pe("Dynamic", (Base,), {})): pass
            "#,
        );
        let item = test.prepare().unwrap();
        insta::assert_snapshot!(
            snapshot(&test.db, &[item]),
            @"vendored://stdlib/builtins.pyi:8615:8619 type :: builtins",
        );
        let supertypes = test.supertypes();
        insta::assert_snapshot!(
            snapshot(&test.db, &supertypes),
            @"vendored://stdlib/builtins.pyi:3608:3614 object :: builtins",
        );
    }

    /// Dynamic classes created via `type()` are not found as subtypes
    /// because they don't create class scopes.
    #[test]
    fn dynamic_class_subtypes_of_class_definition() {
        let test = cursor_test(
            r#"
            class Ba<CURSOR>se:
                pass

            Dynamic = type("Dynamic", (Base,), {})
            "#,
        );

        assert!(test.subtypes().is_empty());
    }

    #[test]
    fn dynamic_class_subtypes_of_dynamic() {
        let test = cursor_test(
            r#"
            Dyn<CURSOR>amic = type("Dynamic", (object,), {})

            class Child(Dynamic): pass
            "#,
        );

        let subtypes = test.subtypes();
        insta::assert_snapshot!(snapshot(&test.db, &subtypes), @"/main.py:49:54 Child :: main");
    }

    /// Like `dynamic_class_prepare_and_supertypes_variable_definition`,
    /// but for named tuples.
    #[test]
    fn namedtuple_prepare_and_supertypes_variable_definition() {
        let test = cursor_test(
            r#"
            from collections import namedtuple

            Dyn<CURSOR>amic = namedtuple("Dynamic", ['x', 'y'])
            "#,
        );

        let item = test.prepare().unwrap();
        insta::assert_snapshot!(snapshot(&test.db, &[item]), @"/main.py:37:44 Dynamic :: main");

        let supertypes = test.supertypes();
        insta::assert_snapshot!(
            snapshot(&test.db, &supertypes),
            @"vendored://stdlib/builtins.pyi:101715:101720 tuple :: builtins",
        );
    }

    /// Like `dynamic_class_prepare_and_supertypes_inline`, but
    /// for named tuples.
    #[test]
    fn namedtuple_prepare_and_supertypes_inline() {
        let test = cursor_test(
            r#"
            from collections import namedtuple

            class Dy<CURSOR>namic(namedtuple("Dynamic", ['x', 'y'])): pass
            "#,
        );
        let item = test.prepare().unwrap();
        insta::assert_snapshot!(snapshot(&test.db, &[item]), @"/main.py:43:50 Dynamic :: main");
        let supertypes = test.supertypes();
        insta::assert_snapshot!(
            snapshot(&test.db, &supertypes),
            @"/main.py:51:84 Dynamic :: main",
        );

        // This fails for a different reason than
        // `dynamic_class_prepare_and_supertypes_inline` in the
        // `type` case. Specifically, `namedtuple` is defined
        // as a function, which our current implementation doesn't
        // recognize as returning a class. So the prepare request
        // doesn't return any items.
        let test = cursor_test(
            r#"
            from collections import namedtuple

            class Dynamic(named<CURSOR>tuple("Dynamic", ['x', 'y'])): pass
            "#,
        );
        assert!(test.prepare().is_none());
    }

    #[test]
    fn namedtuple_subtypes_of_namedtuple() {
        let test = cursor_test(
            r#"
            from collections import namedtuple

            Pa<CURSOR>rent = namedtuple('Parent', ['x', 'y'])
            class Child(Parent): pass
            "#,
        );

        let subtypes = test.subtypes();
        insta::assert_snapshot!(snapshot(&test.db, &subtypes), @"/main.py:85:90 Child :: main");
    }

    /// Named tuples created via `namedtuple()` are not found as subtypes
    /// because they don't create class scopes. Typeshed classes that
    /// inherit from `tuple` (which are defined as regular class
    /// statements) are still found.
    #[test]
    fn namedtuple_subtypes_of_tuple() {
        let test = cursor_test(
            r#"
            from collections import namedtuple

            MyTuple = namedtuple('MyTuple', ['x', 'y'])
            tup<CURSOR>le
            "#,
        );

        let subtypes = test.subtypes();
        let names: Vec<_> = subtypes.iter().map(|s| s.name.as_str()).collect();
        // `MyTuple` is not found because `namedtuple()` doesn't create a class scope.
        assert!(!names.contains(&"MyTuple"));
        // But regular class definitions that inherit from `tuple` are found.
        assert!(names.contains(&"struct_time"));
    }

    /// Re-exports via assignment are not found as subtypes because
    /// we only look at class scopes.
    ///
    /// We could look for these, but when AG tried it, it made perf in
    /// some cases quite slow and produced a lot of false positives.
    #[test]
    fn subtypes_reexport_first_party() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
class Ba<CURSOR>se:
    pass
"#,
            )
            .source(
                "_impl.py",
                r#"
from main import Base

class _Internal(Base):
    pass
"#,
            )
            .source(
                "public.py",
                r#"
from main import Base
from _impl import _Internal

Public = _Internal
"#,
            )
            .build();

        let subtypes = test.subtypes();
        insta::assert_snapshot!(snapshot(&test.db, &subtypes), @"/_impl.py:30:39 _Internal :: _impl");
    }

    /// This is like `subtypes_reexport_first_party`, but results in
    /// not discovering any subtypes because the re-export isn't
    /// discovered. And the original class definition is private.
    #[test]
    fn subtypes_reexport_third_party() {
        let test = CursorTest::builder()
            .with_site_packages()
            .source("main.py", "byte<CURSOR>s")
            .site_packages(
                "thirdparty/__init__.py",
                r#"
                from thirdparty._internal import _bytes_internal
                BytesPublic = _bytes_internal
            "#,
            )
            .site_packages(
                "thirdparty/_internal.py",
                "class _bytes_internal(bytes): pass",
            )
            .build();

        assert!(test.subtypes().is_empty());
    }

    /// This tests that we filter out some subtypes in a way that's consistent
    /// with what we do for auto-import. Specifically, subtypes from
    /// non-first-party tests or private modules.
    #[test]
    fn third_party_filtering() {
        let test = CursorTest::builder()
            .with_site_packages()
            .source("main.py", "byte<CURSOR>s")
            .source("foo.py", "class MyBytes(bytes): pass")
            .site_packages("thirdparty/__init__.py", "class OtherBytes1(bytes): pass")
            .site_packages(
                "thirdparty/_test/__init__.py",
                "class OtherBytes2(bytes): pass",
            )
            .site_packages(
                "thirdparty/_tests/__init__.py",
                "class OtherBytes3(bytes): pass",
            )
            .site_packages(
                "thirdparty/_testing/__init__.py",
                "class OtherBytes4(bytes): pass",
            )
            .site_packages(
                "thirdparty/_foo/__init__.py",
                "class OtherBytes5(bytes): pass",
            )
            .build();

        // We should only see our own subtype and the only third-party
        // subtype that isn't treated as private.
        let subtypes = test.subtypes();
        insta::assert_snapshot!(snapshot(&test.db, &subtypes), @r"
        /src/foo.py:6:13 MyBytes :: foo
        /site-packages/thirdparty/__init__.py:6:17 OtherBytes1 :: thirdparty
        ");
    }

    /// This tests that we don't currently respect `__all__` when returning
    /// subtypes.
    #[test]
    fn subtypes_all_not_respected() {
        let test = CursorTest::builder()
            .with_site_packages()
            .source("main.py", "byte<CURSOR>s")
            .source("foo.py", "class MyBytes(bytes): pass")
            .site_packages(
                "thirdparty/__init__.py",
                r#"
                class OtherBytes1(bytes): pass
                class OtherBytes2(bytes): pass
                __all__ = ['OtherBytes1']
            "#,
            )
            .build();

        // I think ideally we wouldn't include `OtherBytes2` here.
        // Note that pylance doesn't seem to respect `__all__` in
        // this case either.
        let subtypes = test.subtypes();
        insta::assert_snapshot!(snapshot(&test.db, &subtypes), @r"
        /src/foo.py:6:13 MyBytes :: foo
        /site-packages/thirdparty/__init__.py:7:18 OtherBytes1 :: thirdparty
        /site-packages/thirdparty/__init__.py:38:49 OtherBytes2 :: thirdparty
        ");
    }

    fn snapshot(db: &dyn Db, items: &[TypeHierarchyItem]) -> String {
        items
            .iter()
            .map(|item| {
                let mut string = format!(
                    "{path}:{start}:{end} {name}",
                    path = item.file.path(db),
                    start = item.selection_range.start().to_usize(),
                    end = item.selection_range.end().to_usize(),
                    name = item.name,
                );
                if let Some(ref detail) = item.detail {
                    string = format!("{string} :: {detail}");
                }
                string
            })
            .collect::<Vec<String>>()
            .join("\n")
    }

    impl CursorTest {
        fn prepare(&self) -> Option<TypeHierarchyItem> {
            prepare_type_hierarchy(&self.db, self.cursor.file, self.cursor.offset)
        }

        fn supertypes(&self) -> Vec<TypeHierarchyItem> {
            let Some(item) = self.prepare() else {
                return vec![];
            };
            type_hierarchy_supertypes(&self.db, item.file, item.selection_range.start())
        }

        fn subtypes(&self) -> Vec<TypeHierarchyItem> {
            let Some(item) = self.prepare() else {
                return vec![];
            };
            type_hierarchy_subtypes(&self.db, item.file, item.selection_range.start())
        }
    }
}
