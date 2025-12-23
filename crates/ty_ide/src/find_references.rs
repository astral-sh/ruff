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
            let Some(mut reference_results) =
                find_references(&self.db, self.cursor.file, self.cursor.offset, true)
            else {
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

        assert_snapshot!(test.references(), @r"
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

        assert_snapshot!(test.references(), @r"
        info[references]: Found 9 references
          --> main.py:3:5
           |
         2 | def outer_function():
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
        20 |
        21 |     return increment, decrement
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

        assert_snapshot!(test.references(), @r"
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

        assert_snapshot!(test.references(), @r"
        info[references]: Found 4 references
          --> main.py:4:29
           |
         2 | try:
         3 |     x = 1 / 0
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

        assert_snapshot!(test.references(), @r"
        info[references]: Found 3 references
         --> main.py:3:20
          |
        2 | match x:
        3 |     case [a, b] as pattern:
          |                    -------
        4 |         print(f'Matched: {pattern}')
          |                           -------
        5 |         return pattern
          |                -------
        6 |     case _:
        7 |         pass
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

        assert_snapshot!(test.references(), @r"
        info[references]: Found 4 references
         --> main.py:3:29
          |
        2 | match data:
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

        assert_snapshot!(test.references(), @r"
        info[references]: Found 6 references
          --> main.py:2:5
           |
         2 | def my_function():
           |     -----------
         3 |     return 42
           |
          ::: main.py:6:11
           |
         5 | # Call the function multiple times
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

        assert_snapshot!(test.references(), @r"
        info[references]: Found 6 references
          --> main.py:2:7
           |
         2 | class MyClass:
           |       -------
         3 |     def __init__(self):
         4 |         pass
           |
          ::: main.py:7:8
           |
         6 | # Create instances
         7 | obj1 = MyClass()
           |        -------
         8 | obj2 = MyClass()
           |        -------
         9 |
        10 | # Use in type annotations
        11 | def process(instance: MyClass) -> MyClass:
           |                       -------     -------
        12 |     return instance
           |
          ::: main.py:15:7
           |
        14 | # Reference the class itself
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
        5 |     """some docs"""
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
        5 |     """some docs"""
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
        5 |     """some docs"""
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
        5 |     """some docs"""
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
        2 | def my_func(command: str):
        3 |     match command.split():
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
        2 | def my_func(command: str):
        3 |     match command.split():
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
        2 | def my_func(command: str):
        3 |     match command.split():
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
        2 | def my_func(command: str):
        3 |     match command.split():
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
        2 | def my_func(command: str):
        3 |     match command.split():
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
        2 | def my_func(command: str):
        3 |     match command.split():
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

        assert_snapshot!(test.references(), @r"
        info[references]: Found 2 references
          --> main.py:10:30
           |
         8 | def my_func(event: Click):
         9 |     match event:
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

        assert_snapshot!(test.references(), @r"
        info[references]: Found 2 references
          --> main.py:10:30
           |
         8 | def my_func(event: Click):
         9 |     match event:
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

        assert_snapshot!(test.references(), @r#"
        info[references]: Found 3 references
          --> main.py:2:7
           |
         2 | class Click:
           |       -----
         3 |     __match_args__ = ("position", "button")
         4 |     def __init__(self, pos, btn):
           |
          ::: main.py:8:20
           |
         6 |         self.button: str = btn
         7 |
         8 | def my_func(event: Click):
           |                    -----
         9 |     match event:
        10 |         case Click(x, button=ab):
           |              -----
        11 |             x = ab
           |
        "#);
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

        assert_snapshot!(test.references(), @r"
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

        assert_snapshot!(test.references(), @r"
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

        assert_snapshot!(test.references(), @r"
        info[references]: Found 3 references
         --> main.py:3:15
          |
        2 | from typing import Callable
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

        assert_snapshot!(test.references(), @r"
        info[references]: Found 3 references
         --> main.py:3:15
          |
        2 | from typing import Callable
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

        assert_snapshot!(test.references(), @r"
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

        assert_snapshot!(test.references(), @r"
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

        assert_snapshot!(test.references(), @r"
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
        3 |     return x * 2
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

        assert_snapshot!(test.references(), @r"
        info[references]: Found 5 references
         --> main.py:6:19
          |
        4 | def process_model():
        5 |     model = MyModel()
        6 |     value = model.attr
          |                   ----
        7 |     model.attr = 100
          |           ----
        8 |     return model.attr
          |                  ----
          |
         ::: models.py:3:5
          |
        2 | class MyModel:
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
        assert_snapshot!(test.references(), @r"
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
        3 |     def __init__(self, path: str): ...
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

        assert_snapshot!(test.references(), @r"
        info[references]: Found 2 references
         --> main.py:3:20
          |
        2 | import warnings
        3 | import warnings as abc
          |                    ---
        4 |
        5 | x = abc
          |     ---
        6 | y = warnings
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

        assert_snapshot!(test.references(), @r"
        info[references]: Found 2 references
         --> main.py:3:20
          |
        2 | import warnings
        3 | import warnings as abc
          |                    ---
        4 |
        5 | x = abc
          |     ---
        6 | y = warnings
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

        assert_snapshot!(test.references(), @r"
        info[references]: Found 2 references
         --> main.py:2:36
          |
        2 | from warnings import deprecated as xyz
          |                                    ---
        3 | from warnings import deprecated
        4 |
        5 | y = xyz
          |     ---
        6 | z = deprecated
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

        assert_snapshot!(test.references(), @r"
        info[references]: Found 2 references
         --> main.py:2:36
          |
        2 | from warnings import deprecated as xyz
          |                                    ---
        3 | from warnings import deprecated
        4 |
        5 | y = xyz
          |     ---
        6 | z = deprecated
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
        assert_snapshot!(test.references(), @r"
        info[references]: Found 1 references
         --> mypackage/__init__.py:4:5
          |
        2 | from .subpkg.submod import val
        3 |
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

        assert_snapshot!(test.references(), @r"
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

        // TODO: this should also highlight the RHS subpkg in the import
        assert_snapshot!(test.references(), @r"
        info[references]: Found 1 references
         --> mypackage/__init__.py:4:5
          |
        2 | from .subpkg import subpkg
        3 |
        4 | x = subpkg
          |     ------
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
}
