use crate::goto::find_goto_target;
use crate::references::{ReferencesMode, references};
use crate::{Db, ReferenceTarget};
use ruff_db::files::File;
use ruff_text_size::TextSize;
use ty_python_semantic::SemanticModel;

/// Find all references to a symbol at the given position.
/// Search for references across all files in the project.
pub fn find_references(
    db: &dyn Db,
    file: File,
    offset: TextSize,
    include_declaration: bool,
) -> Option<Vec<ReferenceTarget>> {
    let parsed = ruff_db::parsed::parsed_module(db, file);
    let module = parsed.load(db);
    let model = SemanticModel::new(db, file);

    // Get the definitions for the symbol at the cursor position
    let goto_target = find_goto_target(&model, &module, offset)?;

    let mode = if include_declaration {
        ReferencesMode::References
    } else {
        ReferencesMode::ReferencesSkipDeclaration
    };

    references(db, file, &goto_target, mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{CursorTest, IntoDiagnostic, cursor_test};
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span};

    impl CursorTest {
        fn references(&self) -> String {
            self.references_with_include_declaration(true)
        }

        fn references_without_declaration(&self) -> String {
            self.references_with_include_declaration(false)
        }

        fn references_with_include_declaration(&self, include_declaration: bool) -> String {
            let Some(mut reference_results) = find_references(
                &self.db,
                self.cursor.file,
                self.cursor.offset,
                include_declaration,
            ) else {
                return "No references found".to_string();
            };

            if reference_results.is_empty() {
                return "No references found".to_string();
            }

            reference_results.sort_by_key(ReferenceTarget::file);

            self.render_diagnostics([ReferenceResult {
                references: reference_results,
            }])
        }
    }

    struct ReferenceResult {
        references: Vec<ReferenceTarget>,
    }

    impl IntoDiagnostic for ReferenceResult {
        fn into_diagnostic(self) -> Diagnostic {
            let mut main = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("references")),
                Severity::Info,
                format!("Found {} references", self.references.len()),
            );

            for reference in self.references {
                main.annotate(Annotation::secondary(
                    Span::from(reference.file()).with_range(reference.range()),
                ));
            }

            main
        }
    }

    #[test]
    fn parameter_references_in_function() {
        let test = cursor_test(
            "
def calculate_sum(<CURSOR>value: int) -> int:
    doubled = value * 2
    result = value + doubled
    return value

# Call with keyword argument
result = calculate_sum(value=42)
",
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 5 references
         --> main.py:2:19
          |
        2 | def calculate_sum(value: int) -> int:
          |                   -----
        3 |     doubled = value * 2
          |               -----
        4 |     result = value + doubled
          |              -----
        5 |     return value
          |            -----
        6 |
        7 | # Call with keyword argument
        8 | result = calculate_sum(value=42)
          |                        -----
          |
        ");
    }

    #[test]
    fn nonlocal_variable_references() {
        let test = cursor_test(
            "
def outer_function():
    coun<CURSOR>ter = 0

    def increment():
        nonlocal counter
        counter += 1
        return counter

    def decrement():
        nonlocal counter
        counter -= 1
        return counter

    # Use counter in outer scope
    initial = counter
    increment()
    decrement()
    final = counter

    return increment, decrement
",
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 9 references
          --> main.py:3:5
           |
         3 |     counter = 0
           |     -------
         4 |
         5 |     def increment():
         6 |         nonlocal counter
           |                  -------
         7 |         counter += 1
           |         -------
         8 |         return counter
           |                -------
         9 |
        10 |     def decrement():
        11 |         nonlocal counter
           |                  -------
        12 |         counter -= 1
           |         -------
        13 |         return counter
           |                -------
        14 |
        15 |     # Use counter in outer scope
        16 |     initial = counter
           |               -------
        17 |     increment()
        18 |     decrement()
        19 |     final = counter
           |             -------
           |
        ");
    }

    #[test]
    fn global_variable_references() {
        let test = cursor_test(
            "
glo<CURSOR>bal_counter = 0

def increment_global():
    global global_counter
    global_counter += 1
    return global_counter

def decrement_global():
    global global_counter
    global_counter -= 1
    return global_counter

# Use global_counter at module level
initial_value = global_counter
increment_global()
decrement_global()
final_value = global_counter
",
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 9 references
          --> main.py:2:1
           |
         2 | global_counter = 0
           | --------------
         3 |
         4 | def increment_global():
         5 |     global global_counter
           |            --------------
         6 |     global_counter += 1
           |     --------------
         7 |     return global_counter
           |            --------------
         8 |
         9 | def decrement_global():
        10 |     global global_counter
           |            --------------
        11 |     global_counter -= 1
           |     --------------
        12 |     return global_counter
           |            --------------
        13 |
        14 | # Use global_counter at module level
        15 | initial_value = global_counter
           |                 --------------
        16 | increment_global()
        17 | decrement_global()
        18 | final_value = global_counter
           |               --------------
           |
        ");
    }

    #[test]
    fn except_handler_variable_references() {
        let test = cursor_test(
            "
try:
    x = 1 / 0
except ZeroDivisionError as e<CURSOR>rr:
    print(f'Error: {err}')
    return err

try:
    y = 2 / 0
except ValueError as err:
    print(f'Different error: {err}')
",
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 4 references
          --> main.py:4:29
           |
         4 | except ZeroDivisionError as err:
           |                             ---
         5 |     print(f'Error: {err}')
           |                     ---
         6 |     return err
           |            ---
         7 |
         8 | try:
         9 |     y = 2 / 0
        10 | except ValueError as err:
        11 |     print(f'Different error: {err}')
           |                               ---
           |
        ");
    }

    #[test]
    fn pattern_match_as_references() {
        let test = cursor_test(
            "
match x:
    case [a, b] as patter<CURSOR>n:
        print(f'Matched: {pattern}')
        return pattern
    case _:
        pass
",
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 3 references
         --> main.py:3:20
          |
        3 |     case [a, b] as pattern:
          |                    -------
        4 |         print(f'Matched: {pattern}')
          |                           -------
        5 |         return pattern
          |                -------
          |
        ");
    }

    #[test]
    fn pattern_match_mapping_rest_references() {
        let test = cursor_test(
            "
match data:
    case {'a': a, 'b': b, **re<CURSOR>st}:
        print(f'Rest data: {rest}')
        process(rest)
        return rest
",
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 4 references
         --> main.py:3:29
          |
        3 |     case {'a': a, 'b': b, **rest}:
          |                             ----
        4 |         print(f'Rest data: {rest}')
          |                             ----
        5 |         process(rest)
          |                 ----
        6 |         return rest
          |                ----
          |
        ");
    }

    #[test]
    fn function_definition_references() {
        let test = cursor_test(
            "
def my_func<CURSOR>tion():
    return 42

# Call the function multiple times
result1 = my_function()
result2 = my_function()

# Function passed as an argument
callback = my_function

# Function used in different contexts
print(my_function())
value = my_function
",
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 6 references
          --> main.py:2:5
           |
         2 | def my_function():
           |     -----------
           |
          ::: main.py:6:11
           |
         6 | result1 = my_function()
           |           -----------
         7 | result2 = my_function()
           |           -----------
         8 |
         9 | # Function passed as an argument
        10 | callback = my_function
           |            -----------
        11 |
        12 | # Function used in different contexts
        13 | print(my_function())
           |       -----------
        14 | value = my_function
           |         -----------
           |
        ");
    }

    #[test]
    fn overloaded_function_declaration_references_include_all_overloads_and_implementation() {
        let test = CursorTest::builder()
            .source(
                "lib.py",
                r#"
from typing import overload, Any

@overload
def test<CURSOR>() -> None: ...
@overload
def test(a: str) -> str: ...
@overload
def test(a: int) -> int: ...

def test(a: Any) -> Any:
    return a
"#,
            )
            .source(
                "main.py",
                r#"
from lib import test

test("test")
"#,
            )
            .build();

        assert_snapshot!(test.references(), @r#"
        info[references]: Found 6 references
          --> lib.py:5:5
           |
         5 | def test() -> None: ...
           |     ----
         6 | @overload
         7 | def test(a: str) -> str: ...
           |     ----
         8 | @overload
         9 | def test(a: int) -> int: ...
           |     ----
        10 |
        11 | def test(a: Any) -> Any:
           |     ----
           |
          ::: main.py:2:17
           |
         2 | from lib import test
           |                 ----
         3 |
         4 | test("test")
           | ----
           |
        "#);
    }

    #[test]
    fn class_definition_references() {
        let test = cursor_test(
            "
class My<CURSOR>Class:
    def __init__(self):
        pass

# Create instances
obj1 = MyClass()
obj2 = MyClass()

# Use in type annotations
def process(instance: MyClass) -> MyClass:
    return instance

# Reference the class itself
cls = MyClass
",
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 6 references
          --> main.py:2:7
           |
         2 | class MyClass:
           |       -------
           |
          ::: main.py:7:8
           |
         7 | obj1 = MyClass()
           |        -------
         8 | obj2 = MyClass()
           |        -------
         9 |
        10 | # Use in type annotations
        11 | def process(instance: MyClass) -> MyClass:
           |                       -------     -------
           |
          ::: main.py:15:7
           |
        15 | cls = MyClass
           |       -------
           |
        ");
    }

    #[test]
    fn references_string_annotation1() {
        let test = cursor_test(
            r#"
        a: "MyCla<CURSOR>ss" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.references(), @r#"
        info[references]: Found 2 references
         --> main.py:2:5
          |
        2 | a: "MyClass" = 1
          |     -------
        3 |
        4 | class MyClass:
          |       -------
          |
        "#);
    }

    #[test]
    fn references_string_annotation_without_declaration() {
        let test = cursor_test(
            r#"
        a: "MyCla<CURSOR>ss" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.references_without_declaration(), @r#"
        info[references]: Found 1 references
         --> main.py:2:5
          |
        2 | a: "MyClass" = 1
          |     -------
          |
        "#);
    }

    #[test]
    fn references_string_annotation2() {
        let test = cursor_test(
            r#"
        a: "None | MyCl<CURSOR>ass" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.references(), @r#"
        info[references]: Found 2 references
         --> main.py:2:12
          |
        2 | a: "None | MyClass" = 1
          |            -------
        3 |
        4 | class MyClass:
          |       -------
          |
        "#);
    }

    #[test]
    fn references_string_annotation3() {
        let test = cursor_test(
            r#"
        a: "None |<CURSOR> MyClass" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.references(), @"No references found");
    }

    #[test]
    fn references_string_annotation4() {
        let test = cursor_test(
            r#"
        a: "None | MyClass<CURSOR>" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.references(), @r#"
        info[references]: Found 2 references
         --> main.py:2:12
          |
        2 | a: "None | MyClass" = 1
          |            -------
        3 |
        4 | class MyClass:
          |       -------
          |
        "#);
    }

    #[test]
    fn references_string_annotation5() {
        let test = cursor_test(
            r#"
        a: "None | MyClass"<CURSOR> = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.references(), @"No references found");
    }

    #[test]
    fn references_string_annotation_dangling1() {
        let test = cursor_test(
            r#"
        a: "MyCl<CURSOR>ass |" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.references(), @"No references found");
    }

    #[test]
    fn references_string_annotation_dangling2() {
        let test = cursor_test(
            r#"
        a: "MyCl<CURSOR>ass | No" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.references(), @r#"
        info[references]: Found 2 references
         --> main.py:2:5
          |
        2 | a: "MyClass | No" = 1
          |     -------
        3 |
        4 | class MyClass:
          |       -------
          |
        "#);
    }

    #[test]
    fn references_string_annotation_dangling3() {
        let test = cursor_test(
            r#"
        a: "MyClass | N<CURSOR>o" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.references(), @"No references found");
    }

    #[test]
    fn references_string_annotation_recursive() {
        let test = cursor_test(
            r#"
        ab: "a<CURSOR>b"
        "#,
        );

        assert_snapshot!(test.references(), @r#"
        info[references]: Found 2 references
         --> main.py:2:1
          |
        2 | ab: "ab"
          | --   --
          |
        "#);
    }

    #[test]
    fn references_string_annotation_unknown() {
        let test = cursor_test(
            r#"
        x: "foo<CURSOR>bar"
        "#,
        );

        assert_snapshot!(test.references(), @"No references found");
    }

    #[test]
    fn references_match_name_stmt() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", a<CURSOR>b]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.references(), @r#"
        info[references]: Found 2 references
         --> main.py:4:22
          |
        4 |         case ["get", ab]:
          |                      --
        5 |             x = ab
          |                 --
          |
        "#);
    }

    #[test]
    fn references_match_name_binding() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ab]:
                        x = a<CURSOR>b
            "#,
        );

        assert_snapshot!(test.references(), @r#"
        info[references]: Found 2 references
         --> main.py:4:22
          |
        4 |         case ["get", ab]:
          |                      --
        5 |             x = ab
          |                 --
          |
        "#);
    }

    #[test]
    fn references_match_rest_stmt() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", *a<CURSOR>b]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.references(), @r#"
        info[references]: Found 2 references
         --> main.py:4:23
          |
        4 |         case ["get", *ab]:
          |                       --
        5 |             x = ab
          |                 --
          |
        "#);
    }

    #[test]
    fn references_match_rest_binding() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", *ab]:
                        x = a<CURSOR>b
            "#,
        );

        assert_snapshot!(test.references(), @r#"
        info[references]: Found 2 references
         --> main.py:4:23
          |
        4 |         case ["get", *ab]:
          |                       --
        5 |             x = ab
          |                 --
          |
        "#);
    }

    #[test]
    fn references_match_as_stmt() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ("a" | "b") as a<CURSOR>b]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.references(), @r#"
        info[references]: Found 2 references
         --> main.py:4:37
          |
        4 |         case ["get", ("a" | "b") as ab]:
          |                                     --
        5 |             x = ab
          |                 --
          |
        "#);
    }

    #[test]
    fn references_match_as_binding() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ("a" | "b") as ab]:
                        x = a<CURSOR>b
            "#,
        );

        assert_snapshot!(test.references(), @r#"
        info[references]: Found 2 references
         --> main.py:4:37
          |
        4 |         case ["get", ("a" | "b") as ab]:
          |                                     --
        5 |             x = ab
          |                 --
          |
        "#);
    }

    #[test]
    fn references_match_keyword_stmt() {
        let test = cursor_test(
            r#"
            class Click:
                __match_args__ = ("position", "button")
                def __init__(self, pos, btn):
                    self.position: int = pos
                    self.button: str = btn

            def my_func(event: Click):
                match event:
                    case Click(x, button=a<CURSOR>b):
                        x = ab
            "#,
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 2 references
          --> main.py:10:30
           |
        10 |         case Click(x, button=ab):
           |                              --
        11 |             x = ab
           |                 --
           |
        ");
    }

    #[test]
    fn references_match_keyword_binding() {
        let test = cursor_test(
            r#"
            class Click:
                __match_args__ = ("position", "button")
                def __init__(self, pos, btn):
                    self.position: int = pos
                    self.button: str = btn

            def my_func(event: Click):
                match event:
                    case Click(x, button=ab):
                        x = a<CURSOR>b
            "#,
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 2 references
          --> main.py:10:30
           |
        10 |         case Click(x, button=ab):
           |                              --
        11 |             x = ab
           |                 --
           |
        ");
    }

    #[test]
    fn references_match_class_name() {
        let test = cursor_test(
            r#"
            class Click:
                __match_args__ = ("position", "button")
                def __init__(self, pos, btn):
                    self.position: int = pos
                    self.button: str = btn

            def my_func(event: Click):
                match event:
                    case Cl<CURSOR>ick(x, button=ab):
                        x = ab
            "#,
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 3 references
          --> main.py:2:7
           |
         2 | class Click:
           |       -----
           |
          ::: main.py:8:20
           |
         8 | def my_func(event: Click):
           |                    -----
         9 |     match event:
        10 |         case Click(x, button=ab):
           |              -----
           |
        ");
    }

    #[test]
    fn references_match_class_field_name() {
        let test = cursor_test(
            r#"
            class Click:
                __match_args__ = ("position", "button")
                def __init__(self, pos, btn):
                    self.position: int = pos
                    self.button: str = btn

            def my_func(event: Click):
                match event:
                    case Click(x, but<CURSOR>ton=ab):
                        x = ab
            "#,
        );

        assert_snapshot!(test.references(), @"No references found");
    }

    #[test]
    fn references_typevar_name_stmt() {
        let test = cursor_test(
            r#"
            type Alias1[A<CURSOR>B: int = bool] = tuple[AB, list[AB]]
            "#,
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 3 references
         --> main.py:2:13
          |
        2 | type Alias1[AB: int = bool] = tuple[AB, list[AB]]
          |             --                      --       --
          |
        ");
    }

    #[test]
    fn references_typevar_name_binding() {
        let test = cursor_test(
            r#"
            type Alias1[AB: int = bool] = tuple[A<CURSOR>B, list[AB]]
            "#,
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 3 references
         --> main.py:2:13
          |
        2 | type Alias1[AB: int = bool] = tuple[AB, list[AB]]
          |             --                      --       --
          |
        ");
    }

    #[test]
    fn references_typevar_spec_stmt() {
        let test = cursor_test(
            r#"
            from typing import Callable
            type Alias2[**A<CURSOR>B = [int, str]] = Callable[AB, tuple[AB]]
            "#,
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 3 references
         --> main.py:3:15
          |
        3 | type Alias2[**AB = [int, str]] = Callable[AB, tuple[AB]]
          |               --                          --        --
          |
        ");
    }

    #[test]
    fn references_typevar_spec_binding() {
        let test = cursor_test(
            r#"
            from typing import Callable
            type Alias2[**AB = [int, str]] = Callable[A<CURSOR>B, tuple[AB]]
            "#,
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 3 references
         --> main.py:3:15
          |
        3 | type Alias2[**AB = [int, str]] = Callable[AB, tuple[AB]]
          |               --                          --        --
          |
        ");
    }

    #[test]
    fn references_typevar_tuple_stmt() {
        let test = cursor_test(
            r#"
            type Alias3[*A<CURSOR>B = ()] = tuple[tuple[*AB], tuple[*AB]]
            "#,
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 3 references
         --> main.py:2:14
          |
        2 | type Alias3[*AB = ()] = tuple[tuple[*AB], tuple[*AB]]
          |              --                      --          --
          |
        ");
    }

    #[test]
    fn references_typevar_tuple_binding() {
        let test = cursor_test(
            r#"
            type Alias3[*AB = ()] = tuple[tuple[*A<CURSOR>B], tuple[*AB]]
            "#,
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 3 references
         --> main.py:2:14
          |
        2 | type Alias3[*AB = ()] = tuple[tuple[*AB], tuple[*AB]]
          |              --                      --          --
          |
        ");
    }

    #[test]
    fn multi_file_function_references() {
        let test = CursorTest::builder()
            .source(
                "utils.py",
                "
def fun<CURSOR>c(x):
    return x * 2
",
            )
            .source(
                "module.py",
                "
from utils import func

def process_data(data):
    return func(data)
",
            )
            .source(
                "app.py",
                "
from utils import func

class DataProcessor:
    def __init__(self):
        self.multiplier = func

    def process(self, value):
        return func(value)
",
            )
            .build();

        assert_snapshot!(test.references(), @"
        info[references]: Found 6 references
         --> app.py:2:19
          |
        2 | from utils import func
          |                   ----
        3 |
        4 | class DataProcessor:
        5 |     def __init__(self):
        6 |         self.multiplier = func
          |                           ----
        7 |
        8 |     def process(self, value):
        9 |         return func(value)
          |                ----
          |
         ::: module.py:2:19
          |
        2 | from utils import func
          |                   ----
        3 |
        4 | def process_data(data):
        5 |     return func(data)
          |            ----
          |
         ::: utils.py:2:5
          |
        2 | def func(x):
          |     ----
          |
        ");
    }

    #[test]
    fn multi_file_class_attribute_references() {
        let test = CursorTest::builder()
            .source(
                "models.py",
                "
class MyModel:
    a<CURSOR>ttr = 42

    def get_attribute(self):
        return MyModel.attr
",
            )
            .source(
                "main.py",
                "
from models import MyModel

def process_model():
    model = MyModel()
    value = model.attr
    model.attr = 100
    return model.attr
",
            )
            .build();

        assert_snapshot!(test.references(), @"
        info[references]: Found 5 references
         --> main.py:6:19
          |
        6 |     value = model.attr
          |                   ----
        7 |     model.attr = 100
          |           ----
        8 |     return model.attr
          |                  ----
          |
         ::: models.py:3:5
          |
        3 |     attr = 42
          |     ----
        4 |
        5 |     def get_attribute(self):
        6 |         return MyModel.attr
          |                        ----
          |
        ");
    }

    #[test]
    fn multi_file_parameter_references_include_keyword_argument_labels() {
        let test = CursorTest::builder()
            .source(
                "example_rename_2.py",
                "
class ExampleClass:
    def __init__(self, <CURSOR>old_name: str) -> None:
        self.old_name = old_name
",
            )
            .source(
                "example_rename.py",
                r#"
from example_rename_2 import ExampleClass

instance = ExampleClass(old_name="test")
"#,
            )
            .build();

        assert_snapshot!(test.references(), @r#"
        info[references]: Found 3 references
         --> example_rename.py:4:25
          |
        4 | instance = ExampleClass(old_name="test")
          |                         --------
          |
         ::: example_rename_2.py:3:24
          |
        3 |     def __init__(self, old_name: str) -> None:
          |                        --------
        4 |         self.old_name = old_name
          |                         --------
          |
        "#);
    }

    #[test]
    fn references_keyword_argument_typeddict_field() {
        let test = cursor_test(
            "
from typing import TypedDict

class TD(TypedDict):
    f<CURSOR>: int
    g: str

TD(f=1)
",
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 2 references
         --> main.py:5:5
          |
        5 |     f: int
          |     -
        6 |     g: str
        7 |
        8 | TD(f=1)
          |    -
          |
        ");
    }

    #[test]
    fn references_typeddict_field_from_keyword_argument() {
        let test = cursor_test(
            "
from typing import TypedDict

class TD(TypedDict):
    f: int
    g: str

TD(f<CURSOR>=1)
",
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 2 references
         --> main.py:5:5
          |
        5 |     f: int
          |     -
        6 |     g: str
        7 |
        8 | TD(f=1)
          |    -
          |
        ");
    }

    #[test]
    fn references_keyword_argument_namedtuple_field() {
        let test = cursor_test(
            "
from typing import NamedTuple

class NT(NamedTuple):
    f<CURSOR>: int
    g: str

NT(f=1)
",
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 2 references
         --> main.py:5:5
          |
        5 |     f: int
          |     -
        6 |     g: str
        7 |
        8 | NT(f=1)
          |    -
          |
        ");
    }

    #[test]
    fn references_keyword_argument_dataclass_field() {
        let test = cursor_test(
            "
from dataclasses import dataclass

@dataclass
class DC:
    f<CURSOR>: int
    g: str

DC(f=1)
",
        );

        assert_snapshot!(test.references(), @"
        info[references]: Found 2 references
         --> main.py:6:5
          |
        6 |     f: int
          |     -
        7 |     g: str
        8 |
        9 | DC(f=1)
          |    -
          |
        ");
    }

    #[test]
    fn multi_file_function_parameter_references_include_keyword_argument_labels() {
        let test = CursorTest::builder()
            .source(
                "utils.py",
                "
def func(<CURSOR>value: int):
    return value * 2
",
            )
            .source(
                "main.py",
                "
from utils import func

result = func(value=42)
",
            )
            .build();

        assert_snapshot!(test.references(), @"
        info[references]: Found 3 references
         --> main.py:4:15
          |
        4 | result = func(value=42)
          |               -----
          |
         ::: utils.py:2:10
          |
        2 | def func(value: int):
          |          -----
        3 |     return value * 2
          |            -----
          |
        ");
    }

    #[test]
    fn multi_file_parameter_references_from_keyword_argument_include_keyword_argument_labels() {
        let test = CursorTest::builder()
            .source(
                "utils.py",
                "
def func(value: int):
    return value * 2

result = func(value<CURSOR>=42)
",
            )
            .source(
                "caller.py",
                "
from utils import func

result = func(value=1)
",
            )
            .build();

        assert_snapshot!(test.references(), @"
        info[references]: Found 4 references
         --> caller.py:4:15
          |
        4 | result = func(value=1)
          |               -----
          |
         ::: utils.py:2:10
          |
        2 | def func(value: int):
          |          -----
        3 |     return value * 2
          |            -----
        4 |
        5 | result = func(value=42)
          |               -----
          |
        ");
    }

    #[test]
    fn multi_file_async_function_parameter_references_include_keyword_argument_labels() {
        let test = CursorTest::builder()
            .source(
                "utils.py",
                "
async def func(<CURSOR>value: int) -> int:
    return value * 2
",
            )
            .source(
                "main.py",
                "
from utils import func

async def main():
    return await func(value=42)
",
            )
            .build();

        assert_snapshot!(test.references(), @"
        info[references]: Found 3 references
         --> main.py:5:23
          |
        5 |     return await func(value=42)
          |                       -----
          |
         ::: utils.py:2:16
          |
        2 | async def func(value: int) -> int:
          |                -----
        3 |     return value * 2
          |            -----
          |
        ");
    }

    #[test]
    fn multi_file_attribute_references_do_not_include_keyword_argument_labels() {
        let test = CursorTest::builder()
            .source(
                "example_rename_2.py",
                "
class ExampleClass:
    def __init__(self, old_name: str) -> None:
        self.<CURSOR>old_name = old_name
",
            )
            .source(
                "example_rename.py",
                r#"
from example_rename_2 import ExampleClass

instance = ExampleClass(old_name="test")
"#,
            )
            .build();

        assert_snapshot!(test.references(), @"
        info[references]: Found 1 references
         --> example_rename_2.py:4:14
          |
        4 |         self.old_name = old_name
          |              --------
          |
        ");
    }

    #[test]
    fn multi_file_nested_function_parameter_references_do_not_include_keyword_argument_labels() {
        let test = CursorTest::builder()
            .source(
                "outer.py",
                "
def outer():
    def inner(<CURSOR>value: int):
        return value * 2
    return inner
",
            )
            .source(
                "caller.py",
                "
from outer import outer

func = outer()
result = func(value=10)
",
            )
            .build();

        // TODO(parameter-keyword-references): Nested callable owners are intentionally excluded by
        // the external-visibility heuristic (perf/signal tradeoff).
        // Ideal output would also include `caller.py` at `func(value=10)` on `value`.
        assert_snapshot!(test.references(), @"
        info[references]: Found 2 references
         --> outer.py:3:15
          |
        3 |     def inner(value: int):
          |               -----
        4 |         return value * 2
          |                -----
          |
        ");
    }

    #[test]
    fn multi_file_parameter_references_do_not_include_other_parameter_same_name() {
        let test = CursorTest::builder()
            .source(
                "example_rename_2.py",
                "
class ExampleClass:
    def __init__(self, <CURSOR>old_name: str) -> None:
        self.old_name = old_name

    def method(self, old_name: str) -> str:
        return f\"Hello {old_name}\"
",
            )
            .source(
                "example_rename.py",
                r#"
from example_rename_2 import ExampleClass

instance = ExampleClass(old_name="test")
result = instance.method(old_name="world")
"#,
            )
            .build();

        assert_snapshot!(test.references(), @r#"
        info[references]: Found 3 references
         --> example_rename.py:4:25
          |
        4 | instance = ExampleClass(old_name="test")
          |                         --------
          |
         ::: example_rename_2.py:3:24
          |
        3 |     def __init__(self, old_name: str) -> None:
          |                        --------
        4 |         self.old_name = old_name
          |                         --------
          |
        "#);
    }

    #[test]
    fn import_alias_references_should_not_resolve_to_original() {
        let test = CursorTest::builder()
            .source(
                "original.py",
                "
def func():
    pass

func()
",
            )
            .source(
                "importer.py",
                "
from original import func as func_alias

func<CURSOR>_alias()
",
            )
            .build();

        // When finding references to the alias, we should NOT find references
        // to the original function in the original module
        assert_snapshot!(test.references(), @"
        info[references]: Found 2 references
         --> importer.py:2:30
          |
        2 | from original import func as func_alias
          |                              ----------
        3 |
        4 | func_alias()
          | ----------
          |
        ");
    }

    #[test]
    fn stub_target() {
        let test = CursorTest::builder()
            .source(
                "path.pyi",
                r#"
                class Path:
                    def __init__(self, path: str): ...
            "#,
            )
            .source(
                "path.py",
                r#"
                class Path:
                    def __init__(self, path: str):
                        self.path = path
            "#,
            )
            .source(
                "importer.py",
                r#"
                from path import Path<CURSOR>

                a: Path = Path("test")
                "#,
            )
            .build();

        assert_snapshot!(test.references(), @r#"
        info[references]: Found 4 references
         --> importer.py:2:18
          |
        2 | from path import Path
          |                  ----
        3 |
        4 | a: Path = Path("test")
          |    ----   ----
          |
         ::: path.pyi:2:7
          |
        2 | class Path:
          |       ----
          |
        "#);
    }

    #[test]
    fn import_alias() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
                import warnings
                import warnings as <CURSOR>abc

                x = abc
                y = warnings
            "#,
            )
            .build();

        assert_snapshot!(test.references(), @"
        info[references]: Found 2 references
         --> main.py:3:20
          |
        3 | import warnings as abc
          |                    ---
        4 |
        5 | x = abc
          |     ---
          |
        ");
    }

    #[test]
    fn import_alias_use() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
                import warnings
                import warnings as abc

                x = abc<CURSOR>
                y = warnings
            "#,
            )
            .build();

        assert_snapshot!(test.references(), @"
        info[references]: Found 2 references
         --> main.py:3:20
          |
        3 | import warnings as abc
          |                    ---
        4 |
        5 | x = abc
          |     ---
          |
        ");
    }

    #[test]
    fn import_from_alias() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
                from warnings import deprecated as xyz<CURSOR>
                from warnings import deprecated

                y = xyz
                z = deprecated
            "#,
            )
            .build();

        assert_snapshot!(test.references(), @"
        info[references]: Found 2 references
         --> main.py:2:36
          |
        2 | from warnings import deprecated as xyz
          |                                    ---
        3 | from warnings import deprecated
        4 |
        5 | y = xyz
          |     ---
          |
        ");
    }

    #[test]
    fn import_from_alias_use() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
                from warnings import deprecated as xyz
                from warnings import deprecated

                y = xyz<CURSOR>
                z = deprecated
            "#,
            )
            .build();

        assert_snapshot!(test.references(), @"
        info[references]: Found 2 references
         --> main.py:2:36
          |
        2 | from warnings import deprecated as xyz
          |                                    ---
        3 | from warnings import deprecated
        4 |
        5 | y = xyz
          |     ---
          |
        ");
    }

    #[test]
    fn references_submodule_import_from_use() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .subpkg.submod import val

                x = sub<CURSOR>pkg
                "#,
            )
            .source("mypackage/subpkg/__init__.py", r#""#)
            .source(
                "mypackage/subpkg/submod.py",
                r#"
                val: int = 0
                "#,
            )
            .build();

        // TODO(submodule-imports): this should light up both instances of `subpkg`
        assert_snapshot!(test.references(), @"
        info[references]: Found 1 references
         --> mypackage/__init__.py:4:5
          |
        4 | x = subpkg
          |     ------
          |
        ");
    }

    #[test]
    fn references_submodule_import_from_def() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .sub<CURSOR>pkg.submod import val

                x = subpkg
                "#,
            )
            .source("mypackage/subpkg/__init__.py", r#""#)
            .source(
                "mypackage/subpkg/submod.py",
                r#"
                val: int = 0
                "#,
            )
            .build();

        // TODO(submodule-imports): this should light up both instances of `subpkg`
        assert_snapshot!(test.references(), @"No references found");
    }

    #[test]
    fn references_submodule_import_from_wrong_use() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .subpkg.submod import val

                x = sub<CURSOR>mod
                "#,
            )
            .source("mypackage/subpkg/__init__.py", r#""#)
            .source(
                "mypackage/subpkg/submod.py",
                r#"
                val: int = 0
                "#,
            )
            .build();

        // No references is actually correct (or it should only see itself)
        assert_snapshot!(test.references(), @"No references found");
    }

    #[test]
    fn references_submodule_import_from_wrong_def() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .subpkg.sub<CURSOR>mod import val

                x = submod
                "#,
            )
            .source("mypackage/subpkg/__init__.py", r#""#)
            .source(
                "mypackage/subpkg/submod.py",
                r#"
                val: int = 0
                "#,
            )
            .build();

        // No references is actually correct (or it should only see itself)
        assert_snapshot!(test.references(), @"No references found");
    }

    #[test]
    fn references_submodule_import_from_confusing_shadowed_def() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .sub<CURSOR>pkg import subpkg

                x = subpkg
                "#,
            )
            .source(
                "mypackage/subpkg/__init__.py",
                r#"
                subpkg: int = 10
                "#,
            )
            .build();

        // No references is actually correct (or it should only see itself)
        assert_snapshot!(test.references(), @"No references found");
    }

    #[test]
    fn references_submodule_import_from_confusing_real_def() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .subpkg import sub<CURSOR>pkg

                x = subpkg
                "#,
            )
            .source(
                "mypackage/subpkg/__init__.py",
                r#"
                subpkg: int = 10
                "#,
            )
            .build();

        // Includes both the local import binding and the underlying definition target.
        assert_snapshot!(test.references(), @"
        info[references]: Found 3 references
         --> mypackage/__init__.py:2:21
          |
        2 | from .subpkg import subpkg
          |                     ------
        3 |
        4 | x = subpkg
          |     ------
          |
         ::: mypackage/subpkg/__init__.py:2:1
          |
        2 | subpkg: int = 10
          | ------
          |
        ");
    }

    #[test]
    fn references_submodule_import_from_confusing_use() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .subpkg import subpkg

                x = sub<CURSOR>pkg
                "#,
            )
            .source(
                "mypackage/subpkg/__init__.py",
                r#"
                subpkg: int = 10
                "#,
            )
            .build();

        assert_snapshot!(test.references(), @"
        info[references]: Found 3 references
         --> mypackage/__init__.py:2:21
          |
        2 | from .subpkg import subpkg
          |                     ------
        3 |
        4 | x = subpkg
          |     ------
          |
         ::: mypackage/subpkg/__init__.py:2:1
          |
        2 | subpkg: int = 10
          | ------
          |
        ");
    }

    // TODO: Should only return references to the last declaration
    #[test]
    fn declarations() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
                a: str = "test"

                a: int = 10

                print(a<CURSOR>)
                "#,
            )
            .build();

        assert_snapshot!(test.references(), @r#"
        info[references]: Found 3 references
         --> main.py:2:1
          |
        2 | a: str = "test"
          | -
        3 |
        4 | a: int = 10
          | -
        5 |
        6 | print(a)
          |       -
          |
        "#);
    }

    #[test]
    fn without_declaration_excludes_initial_assignment() {
        let test = cursor_test(
            "
x<CURSOR> = 1
print(x)
",
        );

        assert_snapshot!(test.references_without_declaration(), @"
        info[references]: Found 1 references
         --> main.py:3:7
          |
        3 | print(x)
          |       -
          |
        ");
    }

    #[test]
    fn without_declaration_keeps_reassignment_without_declaration() {
        let test = cursor_test(
            "
x = 1
x = 2
print(x<CURSOR>)
",
        );

        assert_snapshot!(test.references_without_declaration(), @"
        info[references]: Found 2 references
         --> main.py:3:1
          |
        3 | x = 2
          | -
        4 | print(x)
          |       -
          |
        ");
    }

    #[test]
    fn without_declaration_keeps_assignment_after_annotation() {
        let test = cursor_test(
            "
x<CURSOR>: int
x = 1
print(x)
",
        );

        assert_snapshot!(test.references_without_declaration(), @"
        info[references]: Found 2 references
         --> main.py:3:1
          |
        3 | x = 1
          | -
        4 | print(x)
          |       -
          |
        ");
    }

    #[test]
    fn without_declaration_excludes_repeated_annotation() {
        let test = cursor_test(
            "
x<CURSOR>: int
x: str
print(x)
",
        );

        assert_snapshot!(test.references_without_declaration(), @"
        info[references]: Found 1 references
         --> main.py:4:7
          |
        4 | print(x)
          |       -
          |
        ");
    }

    #[test]
    fn without_declaration_excludes_type_alias_name() {
        let test = cursor_test(
            "
type Box<CURSOR> = int | None
value: Box
",
        );

        assert_snapshot!(test.references_without_declaration(), @"
        info[references]: Found 1 references
         --> main.py:3:8
          |
        3 | value: Box
          |        ---
          |
        ");
    }

    #[test]
    fn without_declaration_control_flow() {
        let test = cursor_test(
            "
def test(flag: bool):
    if flag:
        x: int = 1
        return

    x = 2
    print(x<CURSOR>)
",
        );

        assert_snapshot!(test.references_without_declaration(), @"
        info[references]: Found 1 references
         --> main.py:8:11
          |
        8 |     print(x)
          |           -
          |
        ");
    }

    #[test]
    fn without_declaration_keeps_binding_when_declaration_is_partial() {
        let test = cursor_test(
            "
def f(flag: bool):
    if flag:
        x: int
    x = 1
    print(x<CURSOR>)
",
        );

        assert_snapshot!(test.references_without_declaration(), @"
        info[references]: Found 2 references
         --> main.py:5:5
          |
        5 |     x = 1
          |     -
        6 |     print(x)
          |           -
          |
        ");
    }

    #[test]
    fn without_declaration_excludes_live_conditional_assignments() {
        let test = cursor_test(
            "
if flag:
    x = 1
else:
    x = 2
print(x<CURSOR>)
",
        );

        assert_snapshot!(test.references_without_declaration(), @"
        info[references]: Found 1 references
         --> main.py:6:7
          |
        6 | print(x)
          |       -
          |
        ");
    }

    #[test]
    fn without_declaration_excludes_initial_attribute_assignment() {
        let test = cursor_test(
            "
class C:
    def __init__(self):
        self.x<CURSOR> = 1

    def f(self):
        print(self.x)
",
        );

        assert_snapshot!(test.references_without_declaration(), @"
        info[references]: Found 1 references
         --> main.py:7:20
          |
        7 |         print(self.x)
          |                    -
          |
        ");
    }

    #[test]
    fn without_declaration_excludes_attribute_assignment_after_base_rebind() {
        let test = cursor_test(
            "
class C:
    def f(self, flag: bool):
        if flag:
            self.x = 1
        else:
            self = C()
        self.x<CURSOR> = 2
        print(self.x)
",
        );

        assert_snapshot!(test.references_without_declaration(), @"
        info[references]: Found 1 references
         --> main.py:9:20
          |
        9 |         print(self.x)
          |                    -
          |
        ");
    }
}
