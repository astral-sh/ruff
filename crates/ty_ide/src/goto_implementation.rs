use crate::goto::{Definitions, GotoTarget, find_goto_target};
use crate::{Db, NavigationTargets, RangedValue};
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::parsed_module;
use ruff_text_size::{Ranged, TextSize};
use ty_python_semantic::{
    ImportAliasResolution, SemanticModel, implementation_definitions_for_attribute,
    implementation_definitions_for_class, implementation_definitions_for_class_references,
    implementation_definitions_for_method,
};

/// Navigate from an attribute access or method declaration to that member and known subclass overrides.
///
/// For an attribute access, this resolves the receiver type and returns the implementation family
/// for that type. The member may be a method or an attribute such as `sound: str = ...`:
///
/// ```py
/// animal.sound
///        ^^^^^
/// ```
///
/// For a method declaration, this uses the containing class as the root and returns that method
/// along with known overrides on subclasses:
///
/// ```py
/// class Animal:
///     def speak(self): ...
///         ^^^^^
/// ```
///
/// For a class declaration, this uses the class as the root and returns it along with its known
/// transitive subclasses:
///
/// ```py
/// class Animal:
///       ^^^^^^
///     pass
/// ```
///
/// For a reference to a class (a base class, annotation, or `Animal()`), this behaves like clicking
/// the class declaration: the referenced class plus its known transitive subclasses. The reference
/// is resolved through its definitions, so a qualified reference such as
/// `module.Animal` or `Outer.Inner` behaves the same as a bare name.
///
/// ```py
/// class Dog(Animal): ...
///           ^^^^^^
/// ```
pub fn goto_implementation(
    db: &dyn Db,
    file: File,
    offset: TextSize,
) -> Option<RangedValue<NavigationTargets>> {
    let module = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    let goto_target = find_goto_target(&model, &module, offset)?;
    let mut candidate_files: Vec<File> = db.project().files(db).iter().copied().collect();
    candidate_files.sort_by(|a, b| a.path(db).as_str().cmp(b.path(db).as_str()));

    let class_reference_implementations = || {
        let resolved_definitions =
            goto_target.definitions(&model, ImportAliasResolution::ResolveAliases);
        let resolved_definitions = resolved_definitions
            .as_ref()
            .map(|definitions| definitions.iter().as_slice())
            .unwrap_or(&[]);

        implementation_definitions_for_class_references(db, resolved_definitions, &candidate_files)
    };

    let implementations = match &goto_target {
        GotoTarget::Expression(expression)
        | GotoTarget::Call {
            callable: expression,
            ..
        } if matches!(
            expression,
            ruff_python_ast::ExprRef::Name(_) | ruff_python_ast::ExprRef::Attribute(_)
        ) =>
        {
            class_reference_implementations().or_else(|| match expression {
                ruff_python_ast::ExprRef::Attribute(attribute) => Some(
                    implementation_definitions_for_attribute(&model, attribute, &candidate_files),
                ),
                _ => None,
            })?
        }
        GotoTarget::StringAnnotationSubexpr { .. } => class_reference_implementations()?,
        GotoTarget::FunctionDef(function) => {
            implementation_definitions_for_method(&model, function, &candidate_files)
        }
        GotoTarget::ClassDef(class) => {
            implementation_definitions_for_class(&model, class, &candidate_files)
        }
        _ => return None,
    };

    if implementations.is_empty() {
        return None;
    }

    let implementation_targets = Definitions::new(implementations)
        .map_stubs_for_implementation(model.db())?
        .into_navigation_targets(model.db());

    Some(RangedValue {
        range: FileRange::new(file, goto_target.range()),
        value: implementation_targets,
    })
}

#[cfg(test)]
mod tests {
    use crate::goto_implementation;
    use crate::tests::{CursorTest, cursor_test};
    use insta::assert_snapshot;

    #[test]
    fn implementation_method_family_from_attribute() {
        let test = cursor_test(
            r#"
            class Animal:
                def speak(self): ...

            class Dog(Animal):
                def speak(self): ...

            class Cat(Animal):
                def speak(self): ...

            def f(animal: Animal):
                animal.spe<CURSOR>ak()
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
          --> main.py:12:12
           |
        12 |     animal.speak()
           |            ^^^^^ Clicking here
           |
        info: Found 3 implementations
         --> main.py:3:9
          |
        3 |     def speak(self): ...
          |         -----
        4 |
        5 | class Dog(Animal):
        6 |     def speak(self): ...
          |         -----
        7 |
        8 | class Cat(Animal):
        9 |     def speak(self): ...
          |         -----
          |
        ");
    }

    #[test]
    fn implementation_abstract_root_method_is_included() {
        let test = cursor_test(
            r#"
            from abc import ABC, abstractmethod

            class Animal(ABC):
                @abstractmethod
                def speak(self) -> str: ...

            class Dog(Animal):
                def speak(self) -> str:
                    return "woof"

            class Cat(Animal):
                def speak(self) -> str:
                    return "meow"

            def f(animal: Animal):
                animal.spe<CURSOR>ak()
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
          --> main.py:17:12
           |
        17 |     animal.speak()
           |            ^^^^^ Clicking here
           |
        info: Found 3 implementations
          --> main.py:6:9
           |
         6 |     def speak(self) -> str: ...
           |         -----
         7 |
         8 | class Dog(Animal):
         9 |     def speak(self) -> str:
           |         -----
           |
          ::: main.py:13:9
           |
        13 |     def speak(self) -> str:
           |         -----
           |
        ");
    }

    #[test]
    fn implementation_transitive_subclass_overrides() {
        let test = cursor_test(
            r#"
            class Animal:
                def speak(self): ...

            class Mammal(Animal):
                pass

            class Dog(Mammal):
                def speak(self): ...

            def f(animal: Animal):
                animal.spe<CURSOR>ak()
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
          --> main.py:12:12
           |
        12 |     animal.speak()
           |            ^^^^^ Clicking here
           |
        info: Found 2 implementations
         --> main.py:3:9
          |
        3 |     def speak(self): ...
          |         -----
          |
         ::: main.py:9:9
          |
        9 |     def speak(self): ...
          |         -----
          |
        ");
    }

    #[test]
    fn implementation_inherited_method_from_concrete_receiver() {
        let test = cursor_test(
            r#"
            class Animal:
                def speak(self): ...

            class Dog(Animal):
                pass

            dog = Dog()
            dog.spe<CURSOR>ak()
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> main.py:9:5
          |
        9 | dog.speak()
          |     ^^^^^ Clicking here
          |
        info: Found 1 implementation
         --> main.py:3:9
          |
        3 |     def speak(self): ...
          |         -----
          |
        ");
    }

    #[test]
    fn implementation_overridden_method_from_concrete_receiver() {
        let test = cursor_test(
            r#"
            class Animal:
                def speak(self): ...

            class Dog(Animal):
                def speak(self): ...

            class Cat(Animal):
                def speak(self): ...

            def f(dog: Dog):
                dog.spe<CURSOR>ak()
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
          --> main.py:12:9
           |
        12 |     dog.speak()
           |         ^^^^^ Clicking here
           |
        info: Found 1 implementation
         --> main.py:6:9
          |
        6 |     def speak(self): ...
          |         -----
          |
        ");
    }

    #[test]
    fn implementation_shadowed_inherited_method_from_concrete_receiver() {
        let test = cursor_test(
            r#"
            class Animal:
                def speak(self): ...

            class Dog(Animal):
                speak = 1

            dog = Dog()
            dog.spe<CURSOR>ak()
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
         --> main.py:9:5
          |
        9 | dog.speak()
          |     ^^^^^ Clicking here
          |
        info: Found 1 implementation
         --> main.py:6:5
          |
        6 |     speak = 1
          |     -----
          |
        ");
    }

    #[test]
    fn implementation_unresolved_root_does_not_scan_subclasses() {
        let test = cursor_test(
            r#"
            class Dog:
                def speak(self): ...

            def f(value: object):
                value.spe<CURSOR>ak()
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"No goto target found");
    }

    #[test]
    fn implementation_overloaded_method_returns_implementation() {
        let test = cursor_test(
            r#"
            from typing import overload

            class Animal:
                @overload
                def speak(self, volume: int) -> int: ...
                @overload
                def speak(self, volume: str) -> str: ...
                def speak(self, volume: int | str) -> int | str:
                    return volume

            def f(animal: Animal):
                animal.spe<CURSOR>ak(1)
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
          --> main.py:13:12
           |
        13 |     animal.speak(1)
           |            ^^^^^ Clicking here
           |
        info: Found 1 implementation
         --> main.py:9:9
          |
        9 |     def speak(self, volume: int | str) -> int | str:
          |         -----
          |
        ");
    }

    #[test]
    fn implementation_inherited_method_from_union_receivers_deduplicates() {
        let test = cursor_test(
            r#"
            class Animal:
                def speak(self): ...

            class Dog(Animal):
                pass

            class Cat(Animal):
                pass

            def f(pet: Dog | Cat):
                pet.spe<CURSOR>ak()
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
          --> main.py:12:9
           |
        12 |     pet.speak()
           |         ^^^^^ Clicking here
           |
        info: Found 1 implementation
         --> main.py:3:9
          |
        3 |     def speak(self): ...
          |         -----
          |
        ");
    }

    #[test]
    fn implementation_typevar_bound_receiver() {
        let test = cursor_test(
            r#"
            class Animal:
                def speak(self): ...

            class Dog(Animal):
                def speak(self): ...

            def f[T: Animal](animal: T):
                animal.spe<CURSOR>ak()
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> main.py:9:12
          |
        9 |     animal.speak()
          |            ^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> main.py:3:9
          |
        3 |     def speak(self): ...
          |         -----
        4 |
        5 | class Dog(Animal):
        6 |     def speak(self): ...
          |         -----
          |
        ");
    }

    #[test]
    fn implementation_subclass_through_import_alias() {
        let test = CursorTest::builder()
            .source(
                "base.py",
                r#"
                class Base:
                    def me<CURSOR>thod(self): ...
                "#,
            )
            .source(
                "aliases.py",
                r#"
                from base import Base as B
                "#,
            )
            .source(
                "child.py",
                r#"
                from aliases import B

                class Child(B):
                    def method(self): ...
                "#,
            )
            .build();

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> base.py:3:9
          |
        3 |     def method(self): ...
          |         ^^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> base.py:3:9
          |
        3 |     def method(self): ...
          |         ------
          |
         ::: child.py:5:9
          |
        5 |     def method(self): ...
          |         ------
          |
        ");
    }

    #[test]
    fn implementation_stub_map_class_method() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import MyClass
x = MyClass(0)
x.act<CURSOR>ion()
",
            )
            .source(
                "mymodule.py",
                r#"
class MyClass:
    def __init__(self, val):
        self.val = val
    def action(self):
        print(self.val)
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
class MyClass:
    def __init__(self, val: bool): ...
    def action(self): ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> main.py:4:3
          |
        4 | x.action()
          |   ^^^^^^ Clicking here
          |
        info: Found 1 implementation
         --> mymodule.py:5:9
          |
        5 |     def action(self):
          |         ------
          |
        ");
    }

    #[test]
    fn implementation_stub_map_overloaded_class_method() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import MyClass
x = MyClass(0)
x.act<CURSOR>ion(1)
",
            )
            .source(
                "mymodule.py",
                r#"
class MyClass:
    def __init__(self, val):
        self.val = val
    def action(self, value):
        return value
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

class MyClass:
    def __init__(self, val: bool): ...
    @overload
    def action(self, value: int) -> int: ...
    @overload
    def action(self, value: str) -> str: ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> main.py:4:3
          |
        4 | x.action(1)
          |   ^^^^^^ Clicking here
          |
        info: Found 1 implementation
         --> mymodule.py:5:9
          |
        5 |     def action(self, value):
          |         ------
          |
        ");
    }

    #[test]
    fn implementation_stub_only_overloaded_class_method() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import MyClass
x = MyClass(0)
x.act<CURSOR>ion(1)
",
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

class MyClass:
    def __init__(self, val: bool): ...
    @overload
    def action(self, value: int) -> int: ...
    @overload
    def action(self, value: str) -> str: ...
"#,
            )
            .build();

        assert_snapshot!(test.goto_implementation(), @"No goto target found");
    }

    #[test]
    fn implementation_method_declaration_root() {
        let test = cursor_test(
            r#"
            class Animal:
                def spe<CURSOR>ak(self): ...

            class Dog(Animal):
                def speak(self): ...
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> main.py:3:9
          |
        3 |     def speak(self): ...
          |         ^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> main.py:3:9
          |
        3 |     def speak(self): ...
          |         -----
        4 |
        5 | class Dog(Animal):
        6 |     def speak(self): ...
          |         -----
          |
        ");
    }

    #[test]
    fn implementation_union_receiver_deduplicates() {
        let test = cursor_test(
            r#"
            class Animal:
                def speak(self): ...

            class Dog(Animal):
                def speak(self): ...

            def f(animal: Animal | Dog):
                animal.spe<CURSOR>ak()
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> main.py:9:12
          |
        9 |     animal.speak()
          |            ^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> main.py:3:9
          |
        3 |     def speak(self): ...
          |         -----
        4 |
        5 | class Dog(Animal):
        6 |     def speak(self): ...
          |         -----
          |
        ");
    }

    #[test]
    fn implementation_unsupported_target() {
        let test = cursor_test(
            r#"
            def function(): ...

            func<CURSOR>tion()
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"No goto target found");
    }

    #[test]
    fn implementation_class_family() {
        let test = cursor_test(
            r#"
            class Anim<CURSOR>al:
                pass

            class Dog(Animal):
                pass

            class Cat(Animal):
                pass
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> main.py:2:7
          |
        2 | class Animal:
          |       ^^^^^^ Clicking here
          |
        info: Found 3 implementations
         --> main.py:2:7
          |
        2 | class Animal:
          |       ------
        3 |     pass
        4 |
        5 | class Dog(Animal):
          |       ---
        6 |     pass
        7 |
        8 | class Cat(Animal):
          |       ---
          |
        ");
    }

    #[test]
    fn implementation_class_no_subclasses() {
        let test = cursor_test(
            r#"
            class Wid<CURSOR>get:
                pass
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> main.py:2:7
          |
        2 | class Widget:
          |       ^^^^^^ Clicking here
          |
        info: Found 1 implementation
         --> main.py:2:7
          |
        2 | class Widget:
          |       ------
          |
        ");
    }

    #[test]
    fn implementation_class_transitive_subclasses() {
        let test = cursor_test(
            r#"
            class Anim<CURSOR>al:
                pass

            class Mammal(Animal):
                pass

            class Dog(Mammal):
                pass
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> main.py:2:7
          |
        2 | class Animal:
          |       ^^^^^^ Clicking here
          |
        info: Found 3 implementations
         --> main.py:2:7
          |
        2 | class Animal:
          |       ------
        3 |     pass
        4 |
        5 | class Mammal(Animal):
          |       ------
        6 |     pass
        7 |
        8 | class Dog(Mammal):
          |       ---
          |
        ");
    }

    #[test]
    fn implementation_class_intermediate_root() {
        let test = cursor_test(
            r#"
            class Animal:
                pass

            class Mam<CURSOR>mal(Animal):
                pass

            class Dog(Mammal):
                pass
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> main.py:5:7
          |
        5 | class Mammal(Animal):
          |       ^^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> main.py:5:7
          |
        5 | class Mammal(Animal):
          |       ------
        6 |     pass
        7 |
        8 | class Dog(Mammal):
          |       ---
          |
        ");
    }

    #[test]
    fn implementation_class_multiple_inheritance() {
        let test = cursor_test(
            r#"
            class Walk<CURSOR>er:
                pass

            class Swimmer:
                pass

            class Amphibian(Walker, Swimmer):
                pass
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> main.py:2:7
          |
        2 | class Walker:
          |       ^^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> main.py:2:7
          |
        2 | class Walker:
          |       ------
          |
         ::: main.py:8:7
          |
        8 | class Amphibian(Walker, Swimmer):
          |       ---------
          |
        ");
    }

    #[test]
    fn implementation_class_diamond_dedup() {
        let test = cursor_test(
            r#"
            class Ba<CURSOR>se:
                pass

            class Left(Base):
                pass

            class Right(Base):
                pass

            class Diamond(Left, Right):
                pass
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> main.py:2:7
          |
        2 | class Base:
          |       ^^^^ Clicking here
          |
        info: Found 4 implementations
          --> main.py:2:7
           |
         2 | class Base:
           |       ----
         3 |     pass
         4 |
         5 | class Left(Base):
           |       ----
         6 |     pass
         7 |
         8 | class Right(Base):
           |       -----
         9 |     pass
        10 |
        11 | class Diamond(Left, Right):
           |       -------
           |
        ");
    }

    #[test]
    fn implementation_class_subclass_through_import_alias() {
        let test = CursorTest::builder()
            .source(
                "base.py",
                r#"
                class Ba<CURSOR>se:
                    pass
                "#,
            )
            .source(
                "aliases.py",
                r#"
                from base import Base as B
                "#,
            )
            .source(
                "child.py",
                r#"
                from aliases import B

                class Child(B):
                    pass
                "#,
            )
            .build();

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> base.py:2:7
          |
        2 | class Base:
          |       ^^^^ Clicking here
          |
        info: Found 2 implementations
         --> base.py:2:7
          |
        2 | class Base:
          |       ----
          |
         ::: child.py:4:7
          |
        4 | class Child(B):
          |       -----
          |
        ");
    }

    #[test]
    fn implementation_class_generic_base() {
        let test = cursor_test(
            r#"
            class Contai<CURSOR>ner[T]:
                pass

            class IntContainer(Container[int]):
                pass
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> main.py:2:7
          |
        2 | class Container[T]:
          |       ^^^^^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> main.py:2:7
          |
        2 | class Container[T]:
          |       ---------
        3 |     pass
        4 |
        5 | class IntContainer(Container[int]):
          |       ------------
          |
        ");
    }

    #[test]
    fn implementation_class_abstract_base_included() {
        let test = cursor_test(
            r#"
            from abc import ABC

            class Anim<CURSOR>al(ABC):
                pass

            class Dog(Animal):
                pass
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> main.py:4:7
          |
        4 | class Animal(ABC):
          |       ^^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> main.py:4:7
          |
        4 | class Animal(ABC):
          |       ------
        5 |     pass
        6 |
        7 | class Dog(Animal):
          |       ---
          |
        ");
    }

    #[test]
    fn implementation_class_protocol_nominal_only() {
        let test = cursor_test(
            r#"
            from typing import Protocol

            class Spea<CURSOR>ker(Protocol):
                def speak(self) -> None: ...

            class Dog:
                def speak(self) -> None: ...

            class Cat(Speaker):
                def speak(self) -> None: ...
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> main.py:4:7
          |
        4 | class Speaker(Protocol):
          |       ^^^^^^^ Clicking here
          |
        info: Found 2 implementations
          --> main.py:4:7
           |
         4 | class Speaker(Protocol):
           |       -------
           |
          ::: main.py:10:7
           |
        10 | class Cat(Speaker):
           |       ---
           |
        ");
    }

    #[test]
    fn implementation_class_reference_in_base_list() {
        let test = cursor_test(
            r#"
            class Animal:
                pass

            class Dog(Anim<CURSOR>al):
                pass
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
         --> main.py:5:11
          |
        5 | class Dog(Animal):
          |           ^^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> main.py:2:7
          |
        2 | class Animal:
          |       ------
        3 |     pass
        4 |
        5 | class Dog(Animal):
          |       ---
          |
        ");
    }

    #[test]
    fn implementation_class_reference_in_annotation() {
        let test = cursor_test(
            r#"
            class Animal:
                pass

            class Dog(Animal):
                pass

            def f(x: Anim<CURSOR>al):
                pass
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
         --> main.py:8:10
          |
        8 | def f(x: Animal):
          |          ^^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> main.py:2:7
          |
        2 | class Animal:
          |       ------
        3 |     pass
        4 |
        5 | class Dog(Animal):
          |       ---
          |
        ");
    }

    #[test]
    fn implementation_class_reference_in_string_annotation() {
        let test = cursor_test(
            r#"
            class Animal:
                pass

            class Dog(Animal):
                pass

            def f(x: "Anim<CURSOR>al"):
                pass
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r#"
        info[goto-implementation]: Go to implementation
         --> main.py:8:11
          |
        8 | def f(x: "Animal"):
          |           ^^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> main.py:2:7
          |
        2 | class Animal:
          |       ------
        3 |     pass
        4 |
        5 | class Dog(Animal):
          |       ---
          |
        "#);
    }

    #[test]
    fn implementation_class_reference_in_instantiation() {
        let test = cursor_test(
            r#"
            class Animal:
                pass

            class Dog(Animal):
                pass

            Anim<CURSOR>al()
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
         --> main.py:8:1
          |
        8 | Animal()
          | ^^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> main.py:2:7
          |
        2 | class Animal:
          |       ------
        3 |     pass
        4 |
        5 | class Dog(Animal):
          |       ---
          |
        ");
    }

    #[test]
    fn implementation_qualified_class_reference_in_base_list() {
        let test = CursorTest::builder()
            .source(
                "animals.py",
                r#"
                class Animal:
                    pass
                "#,
            )
            .source(
                "main.py",
                r#"
                import animals

                class Dog(animals.Anim<CURSOR>al):
                    pass
                "#,
            )
            .build();

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
         --> main.py:4:19
          |
        4 | class Dog(animals.Animal):
          |                   ^^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> animals.py:2:7
          |
        2 | class Animal:
          |       ------
          |
         ::: main.py:4:7
          |
        4 | class Dog(animals.Animal):
          |       ---
          |
        ");
    }

    #[test]
    fn implementation_qualified_class_reference_in_annotation() {
        let test = CursorTest::builder()
            .source(
                "animals.py",
                r#"
                class Animal:
                    pass
                "#,
            )
            .source(
                "main.py",
                r#"
                import animals

                class Dog(animals.Animal):
                    pass

                def f(x: animals.Anim<CURSOR>al):
                    pass
                "#,
            )
            .build();

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
         --> main.py:7:18
          |
        7 | def f(x: animals.Animal):
          |                  ^^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> animals.py:2:7
          |
        2 | class Animal:
          |       ------
          |
         ::: main.py:4:7
          |
        4 | class Dog(animals.Animal):
          |       ---
          |
        ");
    }

    #[test]
    fn implementation_qualified_class_reference_in_instantiation() {
        let test = CursorTest::builder()
            .source(
                "animals.py",
                r#"
                class Animal:
                    pass
                "#,
            )
            .source(
                "main.py",
                r#"
                import animals

                class Dog(animals.Animal):
                    pass

                animals.Anim<CURSOR>al()
                "#,
            )
            .build();

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
         --> main.py:7:9
          |
        7 | animals.Animal()
          |         ^^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> animals.py:2:7
          |
        2 | class Animal:
          |       ------
          |
         ::: main.py:4:7
          |
        4 | class Dog(animals.Animal):
          |       ---
          |
        ");
    }

    #[test]
    fn implementation_nested_class_reference() {
        let test = cursor_test(
            r#"
            class Outer:
                class Inner:
                    pass

            class SubInner(Outer.Inner):
                pass

            def f(x: Outer.In<CURSOR>ner):
                pass
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
         --> main.py:9:16
          |
        9 | def f(x: Outer.Inner):
          |                ^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> main.py:3:11
          |
        3 |     class Inner:
          |           -----
        4 |         pass
        5 |
        6 | class SubInner(Outer.Inner):
          |       --------
          |
        ");
    }

    #[test]
    fn implementation_attribute_bound_to_class() {
        // An attribute that resolves to a class object is a class reference, not a member
        // lookup, matching how a bare name bound to a class behaves.
        let test = cursor_test(
            r#"
            class Dog:
                pass

            class Factory:
                dog_cls = Dog

            def f(factory: Factory):
                factory.dog_<CURSOR>cls
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
         --> main.py:9:13
          |
        9 |     factory.dog_cls
          |             ^^^^^^^ Clicking here
          |
        info: Found 1 implementation
         --> main.py:2:7
          |
        2 | class Dog:
          |       ---
          |
        ");
    }

    #[test]
    fn implementation_class_instance_reference_is_unsupported() {
        // A bare reference to an instance is not a class reference, so it does not resolve to the
        // class implementation family.
        let test = cursor_test(
            r#"
            class Animal:
                pass

            class Dog(Animal):
                pass

            def f(animal: Animal):
                anim<CURSOR>al
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"No goto target found");
    }

    #[test]
    fn implementation_class_stub_mapped_subclass() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
                class Ba<CURSOR>se:
                    pass
                "#,
            )
            .source(
                "mymodule.py",
                r#"
                from main import Base

                class Derived(Base):
                    pass
                "#,
            )
            .source(
                "mymodule.pyi",
                r#"
                from main import Base

                class Derived(Base): ...
                "#,
            )
            .build();

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> main.py:2:7
          |
        2 | class Base:
          |       ^^^^ Clicking here
          |
        info: Found 2 implementations
         --> main.py:2:7
          |
        2 | class Base:
          |       ----
          |
         ::: mymodule.py:4:7
          |
        4 | class Derived(Base):
          |       -------
          |
        ");
    }

    #[test]
    fn implementation_attribute_family_from_base_receiver() {
        let test = cursor_test(
            r#"
            class Animal:
                sound: str = "generic"

            class Dog(Animal):
                sound: str = "woof"

            class Cat(Animal):
                sound: str = "meow"

            def f(animal: Animal):
                animal.so<CURSOR>und
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r#"
        info[goto-implementation]: Go to implementation
          --> main.py:12:12
           |
        12 |     animal.sound
           |            ^^^^^ Clicking here
           |
        info: Found 3 implementations
         --> main.py:3:5
          |
        3 |     sound: str = "generic"
          |     -----
        4 |
        5 | class Dog(Animal):
        6 |     sound: str = "woof"
          |     -----
        7 |
        8 | class Cat(Animal):
        9 |     sound: str = "meow"
          |     -----
          |
        "#);
    }

    #[test]
    fn implementation_attribute_partial_override() {
        let test = cursor_test(
            r#"
            class Animal:
                sound: str = "generic"

            class Dog(Animal):
                sound: str = "woof"

            class Cat(Animal):
                pass

            def f(animal: Animal):
                animal.so<CURSOR>und
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r#"
        info[goto-implementation]: Go to implementation
          --> main.py:12:12
           |
        12 |     animal.sound
           |            ^^^^^ Clicking here
           |
        info: Found 2 implementations
         --> main.py:3:5
          |
        3 |     sound: str = "generic"
          |     -----
        4 |
        5 | class Dog(Animal):
        6 |     sound: str = "woof"
          |     -----
          |
        "#);
    }

    #[test]
    fn implementation_attribute_inherited_from_concrete_receiver() {
        let test = cursor_test(
            r#"
            class Animal:
                sound: str = "generic"

            class Dog(Animal):
                pass

            class Cat(Animal):
                sound: str = "meow"

            def f(dog: Dog):
                dog.so<CURSOR>und
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r#"
        info[goto-implementation]: Go to implementation
          --> main.py:12:9
           |
        12 |     dog.sound
           |         ^^^^^ Clicking here
           |
        info: Found 1 implementation
         --> main.py:3:5
          |
        3 |     sound: str = "generic"
          |     -----
          |
        "#);
    }

    #[test]
    fn implementation_attribute_overridden_from_concrete_receiver() {
        let test = cursor_test(
            r#"
            class Animal:
                sound: str = "generic"

            class Dog(Animal):
                sound: str = "woof"

            class Cat(Animal):
                sound: str = "meow"

            def f(dog: Dog):
                dog.so<CURSOR>und
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r#"
        info[goto-implementation]: Go to implementation
          --> main.py:12:9
           |
        12 |     dog.sound
           |         ^^^^^ Clicking here
           |
        info: Found 1 implementation
         --> main.py:6:5
          |
        6 |     sound: str = "woof"
          |     -----
          |
        "#);
    }

    #[test]
    fn implementation_attribute_plain_assignment() {
        let test = cursor_test(
            r#"
            class Animal:
                sound = "generic"

            class Dog(Animal):
                sound = "woof"

            def f(animal: Animal):
                animal.so<CURSOR>und
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r#"
        info[goto-implementation]: Go to implementation
         --> main.py:9:12
          |
        9 |     animal.sound
          |            ^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> main.py:3:5
          |
        3 |     sound = "generic"
          |     -----
        4 |
        5 | class Dog(Animal):
        6 |     sound = "woof"
          |     -----
          |
        "#);
    }

    #[test]
    fn implementation_attribute_bare_annotation_declaration() {
        let test = cursor_test(
            r#"
            class Animal:
                sound: str

            class Dog(Animal):
                sound: str = "woof"

            def f(animal: Animal):
                animal.so<CURSOR>und
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r#"
        info[goto-implementation]: Go to implementation
         --> main.py:9:12
          |
        9 |     animal.sound
          |            ^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> main.py:3:5
          |
        3 |     sound: str
          |     -----
        4 |
        5 | class Dog(Animal):
        6 |     sound: str = "woof"
          |     -----
          |
        "#);
    }

    #[test]
    fn implementation_attribute_classvar() {
        let test = cursor_test(
            r#"
            from typing import ClassVar

            class Animal:
                sound: ClassVar[str] = "generic"

            class Dog(Animal):
                sound: ClassVar[str] = "woof"

            def f(animal: Animal):
                animal.so<CURSOR>und
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r#"
        info[goto-implementation]: Go to implementation
          --> main.py:11:12
           |
        11 |     animal.sound
           |            ^^^^^ Clicking here
           |
        info: Found 2 implementations
         --> main.py:5:5
          |
        5 |     sound: ClassVar[str] = "generic"
          |     -----
        6 |
        7 | class Dog(Animal):
        8 |     sound: ClassVar[str] = "woof"
          |     -----
          |
        "#);
    }

    #[test]
    fn implementation_attribute_method_and_data_mixed() {
        let test = cursor_test(
            r#"
            class Animal:
                def speak(self): ...

            class Dog(Animal):
                speak = 1

            def f(animal: Animal):
                animal.spe<CURSOR>ak
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
         --> main.py:9:12
          |
        9 |     animal.speak
          |            ^^^^^ Clicking here
          |
        info: Found 2 implementations
         --> main.py:3:9
          |
        3 |     def speak(self): ...
          |         -----
        4 |
        5 | class Dog(Animal):
        6 |     speak = 1
          |     -----
          |
        ");
    }

    #[test]
    fn implementation_attribute_instance_attribute_family() {
        let test = cursor_test(
            r#"
            class Animal:
                def __init__(self):
                    self.sound = "generic"

            class Dog(Animal):
                def __init__(self):
                    self.sound = "woof"

            def f(animal: Animal):
                animal.so<CURSOR>und
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r#"
        info[goto-implementation]: Go to implementation
          --> main.py:11:12
           |
        11 |     animal.sound
           |            ^^^^^ Clicking here
           |
        info: Found 2 implementations
         --> main.py:4:9
          |
        4 |         self.sound = "generic"
          |         ----------
        5 |
        6 | class Dog(Animal):
        7 |     def __init__(self):
        8 |         self.sound = "woof"
          |         ----------
          |
        "#);
    }

    #[test]
    fn implementation_attribute_instance_attribute_from_concrete_receiver() {
        let test = cursor_test(
            r#"
            class Animal:
                def __init__(self):
                    self.sound = "generic"

            class Dog(Animal):
                pass

            class Cat(Animal):
                def __init__(self):
                    self.sound = "meow"

            def f(dog: Dog):
                dog.so<CURSOR>und
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r#"
        info[goto-implementation]: Go to implementation
          --> main.py:14:9
           |
        14 |     dog.sound
           |         ^^^^^ Clicking here
           |
        info: Found 1 implementation
         --> main.py:4:9
          |
        4 |         self.sound = "generic"
          |         ----------
          |
        "#);
    }

    #[test]
    fn implementation_attribute_class_body_and_instance_mixed() {
        let test = cursor_test(
            r#"
            class Animal:
                sound: str = "generic"

            class Dog(Animal):
                def __init__(self):
                    self.sound = "woof"

            def f(animal: Animal):
                animal.so<CURSOR>und
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r#"
        info[goto-implementation]: Go to implementation
          --> main.py:10:12
           |
        10 |     animal.sound
           |            ^^^^^ Clicking here
           |
        info: Found 2 implementations
         --> main.py:3:5
          |
        3 |     sound: str = "generic"
          |     -----
        4 |
        5 | class Dog(Animal):
        6 |     def __init__(self):
        7 |         self.sound = "woof"
          |         ----------
          |
        "#);
    }

    #[test]
    fn implementation_attribute_class_body_takes_priority_over_instance() {
        // When a class defines the attribute both in its body and on `self`, the class-body
        // definition wins for that class, matching the goto-definition lookup.
        let test = cursor_test(
            r#"
            class Animal:
                sound: str = "generic"
                def __init__(self):
                    self.sound = "override"

            def f(animal: Animal):
                animal.so<CURSOR>und
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r#"
        info[goto-implementation]: Go to implementation
         --> main.py:8:12
          |
        8 |     animal.sound
          |            ^^^^^ Clicking here
          |
        info: Found 1 implementation
         --> main.py:3:5
          |
        3 |     sound: str = "generic"
          |     -----
          |
        "#);
    }

    #[test]
    fn implementation_attribute_stub_mapped() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import MyClass
def f(x: MyClass):
    x.so<CURSOR>und
",
            )
            .source(
                "mymodule.py",
                r#"
class MyClass:
    sound: str = "generic"
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
class MyClass:
    sound: str
"#,
            )
            .build();

        assert_snapshot!(test.goto_implementation(), @r#"
        info[goto-implementation]: Go to implementation
         --> main.py:4:7
          |
        4 |     x.sound
          |       ^^^^^ Clicking here
          |
        info: Found 1 implementation
         --> mymodule.py:3:5
          |
        3 |     sound: str = "generic"
          |     -----
          |
        "#);
    }

    #[test]
    fn implementation_attribute_protocol_method_nominal_only() {
        // TODO: the receiver is a `Protocol`, so implementations should be determined by structural
        // inheritance and return all three `speak` definitions (`Speaker`, `Dog`, and `Cat`). We
        // currently use nominal inheritance only and return `Speaker.speak`. See
        // https://github.com/astral-sh/ruff/pull/25410#discussion_r3344203732.
        let test = cursor_test(
            r#"
            from typing import Protocol

            class Speaker(Protocol):
                def speak(self) -> None: ...

            class Dog:
                def speak(self) -> None: ...

            class Cat:
                def speak(self) -> None: ...

            def f(speaker: Speaker):
                speaker.spe<CURSOR>ak()
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
          --> main.py:14:13
           |
        14 |     speaker.speak()
           |             ^^^^^ Clicking here
           |
        info: Found 1 implementation
         --> main.py:5:9
          |
        5 |     def speak(self) -> None: ...
          |         -----
          |
        ");
    }

    #[test]
    fn implementation_attribute_protocol_data_nominal_only() {
        // TODO: as with `implementation_attribute_protocol_method_nominal_only`, structural
        // inheritance should return all three `name` definitions (`Named`, `Dog`, and `Cat`); we
        // currently return only `Named.name`.
        let test = cursor_test(
            r#"
            from typing import Protocol

            class Named(Protocol):
                name: str

            class Dog:
                name: str

            class Cat:
                name: str

            def f(named: Named):
                named.na<CURSOR>me
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
          --> main.py:14:11
           |
        14 |     named.name
           |           ^^^^ Clicking here
           |
        info: Found 1 implementation
         --> main.py:5:5
          |
        5 |     name: str
          |     ----
          |
        ");
    }

    #[test]
    fn implementation_class_unreachable_subclass_excluded() {
        // `ChildFuture` is defined in an unreachable block (the default Python version is well
        // below 3.999), so it must not be returned as an implementation.
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

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
         --> main.py:4:7
          |
        4 | class Base:
          |       ^^^^ Clicking here
          |
        info: Found 2 implementations
         --> main.py:4:7
          |
        4 | class Base:
          |       ----
          |
         ::: main.py:8:11
          |
        8 |     class ChildOld(Base):
          |           --------
          |
        ");
    }

    #[test]
    fn implementation_attribute_unreachable_override_excluded() {
        // `FutureDog.speak` is defined in an unreachable block, so member lookup must not return
        // it as an override.
        let test = cursor_test(
            r#"
            import sys

            class Animal:
                def speak(self): ...

            if sys.version_info >= (3, 5):
                class Dog(Animal):
                    def speak(self): ...

            if sys.version_info >= (3, 999):
                class FutureDog(Animal):
                    def speak(self): ...

            def f(animal: Animal):
                animal.spe<CURSOR>ak()
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
          --> main.py:16:12
           |
        16 |     animal.speak()
           |            ^^^^^ Clicking here
           |
        info: Found 2 implementations
         --> main.py:5:9
          |
        5 |     def speak(self): ...
          |         -----
        6 |
        7 | if sys.version_info >= (3, 5):
        8 |     class Dog(Animal):
        9 |         def speak(self): ...
          |             -----
          |
        ");
    }

    impl CursorTest {
        fn goto_implementation(&self) -> String {
            let Some(targets) = salsa::attach(&self.db, || {
                goto_implementation(&self.db, self.cursor.file, self.cursor.offset)
            }) else {
                return "No goto target found".to_string();
            };

            self.render_diagnostics([crate::goto_definition::test::GotoDiagnostic::new(
                crate::goto_definition::test::GotoAction::Implementation,
                targets,
            )])
        }
    }
}
