//! Finds known implementations of classes and class members.
//!
//! This module implements the `textDocument/implementation` request, commonly exposed as **Go to
//! Implementation** in an editor. It follows nominal inheritance and uses receiver type to decide
//! where to start searching.
//!
//! For example, consider this class hierarchy:
//!
//! ```python
//! class Animal:
//!     sound = "unknown"
//!
//!     def speak(self) -> str:
//!         return self.sound
//!
//! class Dog(Animal):
//!     sound = "woof"
//!
//!     def speak(self) -> str:
//!         return self.sound
//!
//! def make_sound(animal: Animal) -> str:
//!     return animal.speak()
//! ```
//!
//! A request on `animal.speak()` starts from `Animal`, so it returns both `Animal.speak` and
//! `Dog.speak`. A request on a value known to be a `Dog` returns only the implementation selected
//! for `Dog`.
//!
//! # Supported request locations
//!
//! - A method or data attribute use, such as `animal.speak()` or `animal.sound`.
//! - The name in a method declaration, such as `speak` in the definition of `Animal` above. The
//!   containing class becomes the starting point.
//! - The name in a class declaration. The result includes that class and its known subclasses.
//! - A class name used as a base class, type annotation, or constructor call. Qualified names such
//!   as `module.Animal` and `Outer.Inner` are also supported.
//!
//! # Selecting results
//!
//! - An overloaded method resolves to its implementation body when one is available.
//! - Reading, assigning, or deleting a property selects its getter, setter, or deleter (the
//!   corresponding property accessor).
//! - A declaration in a `.pyi` stub file maps to the corresponding source definition when
//!   possible.
//! - A class or member definition that cannot run in the configured Python environment is
//!   excluded. A request directly on an unreachable class, method, property getter, setter, or
//!   deleter returns no result.
//!
//! # Limits
//!
//! - Classes are not discovered just because they provide the methods required by a
//!   `typing.Protocol` (structural subtyping); they must explicitly inherit from that protocol.
//! - Properties created with Python's built-in `property` are recognized. Other objects that
//!   customize what happens when an attribute is read, assigned, or deleted (descriptors) are not
//!   interpreted as properties.

use crate::goto::{Definitions, GotoTarget, find_goto_target};
use crate::{Db, NavigationTarget, NavigationTargets, RangedValue};
use rayon::prelude::*;
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::parsed_module;
use ruff_text_size::{Ranged, TextSize};
use ty_project::parallel::ParallelIteratorExt;
use ty_python_core::ProgramFile;
use ty_python_semantic::{
    ImplementationsFinder, ImportAliasResolution, ResolvedDefinition, SemanticModel,
};

/// Returns the known implementations for the supported target at `offset`.
///
/// Returns `None` when the cursor is not on a supported target or no implementation can be
/// identified.
pub fn goto_implementation(
    db: &dyn Db,
    file: ProgramFile<'_>,
    offset: TextSize,
) -> Option<RangedValue<NavigationTargets>> {
    let module = parsed_module(db, file.python_file(db)).load(db);
    let model = SemanticModel::new(db, file);
    let goto_target = find_goto_target(&model, &module, offset)?;
    let finder = prepare_implementations_finder_for_goto_target(&model, &goto_target)?;
    let source_file = file.file(db);
    let program = file.program(db);

    let mut candidate_files: Vec<File> = db
        .project()
        .files(db)
        .iter()
        .filter(|candidate| *candidate != source_file)
        .collect();
    candidate_files.push(source_file);

    let batches = candidate_files
        .into_par_iter()
        .map_with_db(db, |db, file| {
            let file = ProgramFile::new(db, file, program);
            let definitions = finder.implementations_for_file(db, file);
            definitions_to_implementation_targets(db, definitions)
        })
        .collect::<Vec<_>>();

    let mut implementation_targets =
        definitions_to_implementation_targets(db, finder.into_initial_definitions());
    implementation_targets.extend(batches.into_iter().flatten());

    if implementation_targets.is_empty() {
        return None;
    }

    let implementation_targets = implementation_targets.into_iter().collect();

    Some(RangedValue {
        range: FileRange::new(source_file, goto_target.range()),
        value: implementation_targets,
    })
}

/// Select and prepare the appropriate `ImplementationsFinder` for `goto_target`.
fn prepare_implementations_finder_for_goto_target<'db>(
    model: &SemanticModel<'db>,
    goto_target: &GotoTarget<'_>,
) -> Option<ImplementationsFinder<'db>> {
    let db = model.db();
    let env = model.program_environment();
    match goto_target {
        GotoTarget::Expression(expression)
        | GotoTarget::Call {
            callable: expression,
            ..
        } if matches!(
            expression,
            ruff_python_ast::ExprRef::Name(_) | ruff_python_ast::ExprRef::Attribute(_)
        ) =>
        {
            goto_target
                .expression_definitions(model, ImportAliasResolution::ResolveAliases)
                .and_then(|definitions| {
                    ImplementationsFinder::for_class_reference(
                        db,
                        &env,
                        definitions.iter().as_slice(),
                    )
                })
                .or_else(|| match expression {
                    ruff_python_ast::ExprRef::Attribute(attribute) => {
                        ImplementationsFinder::for_attribute(model, attribute)
                    }
                    _ => None,
                })
        }
        GotoTarget::StringAnnotationSubexpr { .. } => goto_target
            .definitions(model, ImportAliasResolution::ResolveAliases)
            .and_then(|definitions| {
                ImplementationsFinder::for_class_reference(db, &env, definitions.iter().as_slice())
            }),
        GotoTarget::FunctionDef(function) => ImplementationsFinder::for_method(model, function),
        GotoTarget::ClassDef(class) => ImplementationsFinder::for_class(model, class),
        _ => None,
    }
}

fn definitions_to_implementation_targets(
    db: &dyn Db,
    definitions: Vec<ResolvedDefinition>,
) -> Vec<NavigationTarget> {
    Definitions::new(definitions)
        .map_stubs_for_implementation(db)
        .map(|definitions| {
            definitions
                .into_navigation_targets(db)
                .into_iter()
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use crate::goto_implementation;
    use crate::tests::{CursorTest, cursor_test};
    use insta::assert_snapshot;
    use ruff_db::system::SystemPathBuf;
    use ty_project::Db as _;

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
        info: Found 1 implementation
         --> main.py:3:9
          |
        3 |     def speak(self): ...
          |         -----
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
        info: Found 1 implementation
         --> main.py:6:9
          |
        6 |     def speak(self): ...
          |         -----
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
        info: Found 1 implementation
         --> main.py:6:5
          |
        6 |     speak = 1
          |     -----
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
        info: Found 1 implementation
         --> main.py:9:9
          |
        9 |     def speak(self, volume: int | str) -> int | str:
          |         -----
        ");
    }

    #[test]
    fn implementation_overload_only_root_scans_subclasses() {
        let test = cursor_test(
            r#"
            from typing import overload

            class Animal:
                @overload
                def speak(self, volume: int) -> int: ...
                @overload
                def speak(self, volume: str) -> str: ...

            class Dog(Animal):
                def speak(self, volume: int | str) -> int | str:
                    return volume

            def f(animal: Animal):
                animal.spe<CURSOR>ak(1)
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
          --> main.py:15:12
           |
        15 |     animal.speak(1)
           |            ^^^^^ Clicking here
        info: Found 1 implementation
          --> main.py:11:9
           |
        11 |     def speak(self, volume: int | str) -> int | str:
           |         -----
        ");
    }

    #[test]
    fn implementation_property_setter_definition() {
        let test = cursor_test(
            r#"
            class Base:
                @property
                def value(self) -> int: ...

                @value.setter
                def value<CURSOR>(self, value: int) -> None: ...

                @value.deleter
                def value(self) -> None: ...

            class Child(Base):
                @property
                def value(self) -> int: ...

                @value.setter
                def value(self, value: int) -> None: ...

                @value.deleter
                def value(self) -> None: ...
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
         --> main.py:7:9
          |
        7 |     def value(self, value: int) -> None: ...
          |         ^^^^^ Clicking here
        info: Found 2 implementations
          --> main.py:7:9
           |
         7 |     def value(self, value: int) -> None: ...
           |         -----
           |
          ::: main.py:17:9
           |
        17 |     def value(self, value: int) -> None: ...
           |         -----
        ");
    }

    #[test]
    fn implementation_property_read() {
        let test = cursor_test(
            r#"
            class Base:
                @property
                def value(self) -> int: ...

                @value.setter
                def value(self, value: int) -> None: ...

                @value.deleter
                def value(self) -> None: ...

            class Child(Base):
                @property
                def value(self) -> int: ...

                @value.setter
                def value(self, value: int) -> None: ...

                @value.deleter
                def value(self) -> None: ...

            def f(base: Base):
                return base.value<CURSOR>
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
          --> main.py:23:17
           |
        23 |     return base.value
           |                 ^^^^^ Clicking here
        info: Found 2 implementations
          --> main.py:4:9
           |
         4 |     def value(self) -> int: ...
           |         -----
           |
          ::: main.py:14:9
           |
        14 |     def value(self) -> int: ...
           |         -----
        ");
    }

    #[test]
    fn implementation_property_write() {
        let test = cursor_test(
            r#"
            class Base:
                @property
                def value(self) -> int: ...

                @value.setter
                def value(self, value: int) -> None: ...

                @value.deleter
                def value(self) -> None: ...

            class Child(Base):
                @property
                def value(self) -> int: ...

                @value.setter
                def value(self, value: int) -> None: ...

                @value.deleter
                def value(self) -> None: ...

            def f(base: Base, value: int):
                base.value<CURSOR> = value
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
          --> main.py:23:10
           |
        23 |     base.value = value
           |          ^^^^^ Clicking here
        info: Found 2 implementations
          --> main.py:7:9
           |
         7 |     def value(self, value: int) -> None: ...
           |         -----
           |
          ::: main.py:17:9
           |
        17 |     def value(self, value: int) -> None: ...
           |         -----
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
        info: Found 1 implementation
         --> main.py:3:9
          |
        3 |     def speak(self): ...
          |         -----
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
        info: Found 2 implementations
         --> main.py:3:9
          |
        3 |     def speak(self): ...
          |         -----
        4 |
        5 | class Dog(Animal):
        6 |     def speak(self): ...
          |         -----
        ");
    }

    #[test]
    fn implementation_classmethod_receiver() {
        let test = cursor_test(
            r#"
            class Animal:
                @classmethod
                def speak(cls): ...

                @classmethod
                def call(cls):
                    cls.speak<CURSOR>()

            class Dog(Animal):
                @classmethod
                def speak(cls): ...
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
         --> main.py:8:13
          |
        8 |         cls.speak()
          |             ^^^^^ Clicking here
        info: Found 2 implementations
          --> main.py:4:9
           |
         4 |     def speak(cls): ...
           |         -----
           |
          ::: main.py:12:9
           |
        12 |     def speak(cls): ...
           |         -----
        ");
    }

    #[test]
    fn implementation_typevar_bound_class_object_receiver() {
        let test = cursor_test(
            r#"
            class Animal:
                @classmethod
                def speak(cls): ...

            class Dog(Animal):
                @classmethod
                def speak(cls): ...

            def f[T: Animal](cls: type[T]):
                cls.speak<CURSOR>()
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r"
        info[goto-implementation]: Go to implementation
          --> main.py:11:9
           |
        11 |     cls.speak()
           |         ^^^^^ Clicking here
        info: Found 2 implementations
         --> main.py:4:9
          |
        4 |     def speak(cls): ...
          |         -----
        5 |
        6 | class Dog(Animal):
        7 |     @classmethod
        8 |     def speak(cls): ...
          |         -----
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
        ");
    }

    #[test]
    fn implementation_parallel_candidate_batches_preserve_order() {
        let test = CursorTest::builder()
            .source(
                "base.py",
                r#"
                class Base:
                    def me<CURSOR>thod(self): ...
                "#,
            )
            .source(
                "z_child.py",
                r#"
                from base import Base

                class ZChild(Base):
                    def method(self): ...
                "#,
            )
            .source(
                "a_child.py",
                r#"
                from base import Base

                class AChild(Base):
                    def method(self): ...
                "#,
            )
            .build();

        let targets = salsa::attach(&test.db, || {
            goto_implementation(
                &test.db,
                test.program_file(test.cursor.file),
                test.cursor.offset,
            )
            .expect("implementation targets")
        });
        let paths = targets
            .into_iter()
            .map(|target| target.file().path(&test.db).to_string())
            .collect::<Vec<_>>();

        assert_eq!(paths, ["/base.py", "/z_child.py", "/a_child.py"]);
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
        info: Found 1 implementation
         --> mymodule.py:5:9
          |
        5 |     def action(self):
          |         ------
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
        info: Found 1 implementation
         --> mymodule.py:5:9
          |
        5 |     def action(self, value):
          |         ------
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
        info: Found 2 implementations
         --> main.py:3:9
          |
        3 |     def speak(self): ...
          |         -----
        4 |
        5 | class Dog(Animal):
        6 |     def speak(self): ...
          |         -----
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
            from abc import ABC

            class Anim<CURSOR>al(ABC):
                pass

            class Dog(Animal):
                pass

            class Cat(Animal):
                pass
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> main.py:4:7
          |
        4 | class Animal(ABC):
          |       ^^^^^^ Clicking here
        info: Found 3 implementations
          --> main.py:4:7
           |
         4 | class Animal(ABC):
           |       ------
         5 |     pass
         6 |
         7 | class Dog(Animal):
           |       ---
         8 |     pass
         9 |
        10 | class Cat(Animal):
           |       ---
        ");
    }

    #[test]
    fn implementation_class_family_in_request_file_excluded_from_project() {
        let mut test = CursorTest::builder()
            .source(
                "main.py",
                r#"
                class Anim<CURSOR>al:
                    pass

                class Dog(Animal):
                    pass
                "#,
            )
            .source("included.py", "")
            .build();

        test.db
            .project()
            .set_included_paths(&mut test.db, vec![SystemPathBuf::from("/included.py")]);

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
         --> main.py:2:7
          |
        2 | class Animal:
          |       ^^^^^^ Clicking here
        info: Found 2 implementations
         --> main.py:2:7
          |
        2 | class Animal:
          |       ------
        3 |     pass
        4 |
        5 | class Dog(Animal):
          |       ---
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
        info: Found 1 implementation
         --> main.py:2:7
          |
        2 | class Widget:
          |       ------
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
        info: Found 2 implementations
         --> main.py:5:7
          |
        5 | class Mammal(Animal):
          |       ------
        6 |     pass
        7 |
        8 | class Dog(Mammal):
          |       ---
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
        info: Found 2 implementations
         --> main.py:2:7
          |
        2 | class Container[T]:
          |       ---------
        3 |     pass
        4 |
        5 | class IntContainer(Container[int]):
          |       ------------
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
        info: Found 2 implementations
         --> main.py:2:7
          |
        2 | class Animal:
          |       ------
        3 |     pass
        4 |
        5 | class Dog(Animal):
          |       ---
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
        info: Found 2 implementations
         --> main.py:2:7
          |
        2 | class Animal:
          |       ------
        3 |     pass
        4 |
        5 | class Dog(Animal):
          |       ---
        "#);
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
        ");
    }

    #[test]
    fn implementation_class_call_with_assigned_constructor() {
        let test = cursor_test(
            r#"
            def init(self):
                pass

            class Base:
                __init__ = init

            class Child(Base):
                pass

            Ba<CURSOR>se()
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
          --> main.py:11:1
           |
        11 | Base()
           | ^^^^ Clicking here
        info: Found 2 implementations
         --> main.py:5:7
          |
        5 | class Base:
          |       ----
        6 |     __init__ = init
        7 |
        8 | class Child(Base):
          |       -----
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
        info: Found 2 implementations
         --> main.py:3:11
          |
        3 |     class Inner:
          |           -----
        4 |         pass
        5 |
        6 | class SubInner(Outer.Inner):
          |       --------
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
        info: Found 1 implementation
         --> main.py:2:7
          |
        2 | class Dog:
          |       ---
        ");
    }

    #[test]
    fn implementation_mixed_class_value_attribute_uses_member_bindings() {
        let test = cursor_test(
            r#"
            flag: bool

            class Dog:
                pass

            class Puppy(Dog):
                pass

            class Factory:
                item: type[Dog] | int
                if flag:
                    item = Dog
                else:
                    item = 0

            factory = Factory()
            factory.it<CURSOR>em
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
          --> main.py:18:9
           |
        18 | factory.item
           |         ^^^^ Clicking here
        info: Found 3 implementations
          --> main.py:11:5
           |
        11 |     item: type[Dog] | int
           |     ----
        12 |     if flag:
        13 |         item = Dog
           |         ----
        14 |     else:
        15 |         item = 0
           |         ----
        ");
    }

    #[test]
    fn implementation_mixed_class_and_module_binding_is_unsupported() {
        let test = CursorTest::builder()
            .source("flag_source.py", "flag: bool")
            .source("helper.py", "value = 1")
            .source(
                "main.py",
                r#"
                import flag_source

                class Dog:
                    pass

                class Puppy(Dog):
                    pass

                if flag_source.flag:
                    import helper as Item
                else:
                    Item = Dog

                It<CURSOR>em
                "#,
            )
            .build();

        assert_snapshot!(test.goto_implementation(), @"No goto target found");
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
        info: Found 2 implementations
         --> main.py:3:5
          |
        3 |     sound = "generic"
          |     -----
        4 |
        5 | class Dog(Animal):
        6 |     sound = "woof"
          |     -----
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
        info: Found 2 implementations
         --> main.py:3:5
          |
        3 |     sound: str
          |     -----
        4 |
        5 | class Dog(Animal):
        6 |     sound: str = "woof"
          |     -----
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
        info: Found 2 implementations
         --> main.py:3:9
          |
        3 |     def speak(self): ...
          |         -----
        4 |
        5 | class Dog(Animal):
        6 |     speak = 1
          |     -----
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
        info: Found 1 implementation
         --> main.py:4:9
          |
        4 |         self.sound = "generic"
          |         ----------
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
        info: Found 1 implementation
         --> main.py:3:5
          |
        3 |     sound: str = "generic"
          |     -----
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
        info: Found 1 implementation
         --> mymodule.py:3:5
          |
        3 |     sound: str = "generic"
          |     -----
        "#);
    }

    #[test]
    fn implementation_attribute_protocol_method_nominal_only() {
        // TODO: the receiver is a `Protocol`, so implementations should be determined by structural
        // subtyping and return all three `speak` definitions (`Speaker`, `Dog`, and `Cat`). We
        // currently use nominal inheritance only and return `Speaker.speak` and `Cat.speak`. See
        // https://github.com/astral-sh/ruff/pull/25410#discussion_r3344203732.
        let test = cursor_test(
            r#"
            from typing import Protocol

            class Speaker(Protocol):
                def speak(self) -> None: ...

            class Dog:
                def speak(self) -> None: ...

            class Cat(Speaker):
                def speak(self) -> None: ...

            def f(speaker: Speaker):
                speaker.spe<CURSOR>ak()
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"
        info[goto-implementation]: Go to implementation
          --> main.py:14:13
           |
        14 |     speaker.speak()
           |             ^^^^^ Clicking here
        info: Found 2 implementations
          --> main.py:5:9
           |
         5 |     def speak(self) -> None: ...
           |         -----
           |
          ::: main.py:11:9
           |
        11 |     def speak(self) -> None: ...
           |         -----
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
        ");
    }

    #[test]
    fn implementation_attribute_unreachable_method_in_reachable_class_excluded() {
        let test = cursor_test(
            r#"
            import sys

            class Animal:
                def speak(self): ...

            class Dog(Animal):
                if sys.version_info >= (3, 999):
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
        info: Found 1 implementation
         --> main.py:5:9
          |
        5 |     def speak(self): ...
          |         -----
        ");
    }

    #[test]
    fn implementation_attribute_unreachable_data_in_reachable_class_excluded() {
        let test = cursor_test(
            r#"
            import sys

            class Animal:
                sound: str = "generic"

            class Dog(Animal):
                def __init__(self):
                    if sys.version_info >= (3, 999):
                        self.sound = "woof"

            def f(animal: Animal):
                animal.so<CURSOR>und
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @r#"
        info[goto-implementation]: Go to implementation
          --> main.py:13:12
           |
        13 |     animal.sound
           |            ^^^^^ Clicking here
        info: Found 1 implementation
         --> main.py:5:5
          |
        5 |     sound: str = "generic"
          |     -----
        "#);
    }

    #[test]
    fn implementation_unreachable_class_declaration_is_unsupported() {
        let test = cursor_test(
            r#"
            import sys

            if sys.version_info >= (3, 999):
                class Anim<CURSOR>al:
                    pass

                class Child(Animal):
                    pass
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"No goto target found");
    }

    #[test]
    fn implementation_unreachable_class_reference_is_unsupported() {
        let test = cursor_test(
            r#"
            import sys

            if sys.version_info >= (3, 999):
                class Animal:
                    pass

                class Child(Animal):
                    pass

                value: Anim<CURSOR>al
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"No goto target found");
    }

    #[test]
    fn implementation_unreachable_method_declaration_is_unsupported() {
        let test = cursor_test(
            r#"
            import sys

            class Animal:
                def speak(self): ...

            class Dog(Animal):
                if sys.version_info >= (3, 999):
                    def spe<CURSOR>ak(self): ...

            class Pup(Dog):
                def speak(self): ...
            "#,
        );

        assert_snapshot!(test.goto_implementation(), @"No goto target found");
    }

    impl CursorTest {
        fn goto_implementation(&self) -> String {
            let Some(targets) = salsa::attach(&self.db, || {
                goto_implementation(
                    &self.db,
                    self.program_file(self.cursor.file),
                    self.cursor.offset,
                )
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
