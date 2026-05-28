use crate::goto::{Definitions, GotoTarget, find_goto_target};
use crate::{Db, NavigationTargets, RangedValue};
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::parsed_module;
use ruff_text_size::{Ranged, TextSize};
use ty_python_semantic::{
    SemanticModel, implementation_definitions_for_attribute, implementation_definitions_for_method,
};

pub fn goto_implementation(
    db: &dyn Db,
    file: File,
    offset: TextSize,
) -> Option<RangedValue<NavigationTargets>> {
    let module = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    let goto_target = find_goto_target(&model, &module, offset)?;

    let implementations = match &goto_target {
        GotoTarget::Expression(ruff_python_ast::ExprRef::Attribute(attribute))
        | GotoTarget::Call {
            callable: ruff_python_ast::ExprRef::Attribute(attribute),
            ..
        } => implementation_definitions_for_attribute(&model, attribute),
        GotoTarget::FunctionDef(function) => {
            implementation_definitions_for_method(&model, function)
        }
        _ => return None,
    };

    if implementations.is_empty() {
        return None;
    }

    let implementation_targets =
        Definitions::new(implementations).into_navigation_targets(model.db());

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
