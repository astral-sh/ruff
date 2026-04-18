use crate::{Mode, ParseOptions, parse, parse_expression, parse_module};

#[test]
fn test_modes() {
    let source = "a[0][1][2][3][4]";

    assert!(parse(source, ParseOptions::from(Mode::Expression)).is_ok());
    assert!(parse(source, ParseOptions::from(Mode::Module)).is_ok());
}

#[test]
fn test_expr_mode_invalid_syntax1() {
    let source = "first second";
    let error = parse_expression(source).unwrap_err();

    insta::assert_debug_snapshot!(error);
}

#[test]
fn test_expr_mode_invalid_syntax2() {
    let source = r"first

second
";
    let error = parse_expression(source).unwrap_err();

    insta::assert_debug_snapshot!(error);
}

#[test]
fn test_expr_mode_invalid_syntax3() {
    let source = r"first

second

third
";
    let error = parse_expression(source).unwrap_err();

    insta::assert_debug_snapshot!(error);
}

#[test]
fn test_expr_mode_valid_syntax() {
    let source = "first

";
    let parsed = parse_expression(source).unwrap();

    insta::assert_debug_snapshot!(parsed.expr());
}

#[test]
fn test_unicode_aliases() {
    // https://github.com/RustPython/RustPython/issues/4566
    let source = r#"x = "\N{BACKSPACE}another cool trick""#;
    let suite = parse_module(source).unwrap().into_suite();

    insta::assert_debug_snapshot!(suite);
}

#[test]
fn test_ipython_escape_commands() {
    let parsed = parse(
        r"
# Normal Python code
(
    a
    %
    b
)

# Dynamic object info
??a.foo
?a.foo
?a.foo?
??a.foo()??

# Line magic
%timeit a = b
%timeit foo(b) % 3
%alias showPath pwd && ls -a
%timeit a =\
  foo(b); b = 2
%matplotlib --inline
%matplotlib \
    --inline

# System shell access
!pwd && ls -a | sed 's/^/\    /'
!pwd \
  && ls -a | sed 's/^/\\    /'
!!cd /Users/foo/Library/Application\ Support/

# Let's add some Python code to make sure that earlier escapes were handled
# correctly and that we didn't consume any of the following code as a result
# of the escapes.
def foo():
    return (
        a
        !=
        b
    )

# Transforms into `foo(..)`
/foo 1 2
;foo 1 2
,foo 1 2

# Indented escape commands
for a in range(5):
    !ls

p1 = !pwd
p2: str = !pwd
foo = %foo \
    bar
bar = %foo?
baz = !pwd?

% foo
foo = %foo  # comment

# Help end line magics
foo?
foo.bar??
foo.bar.baz?
foo[0]??
foo[0][1]?
foo.bar[0].baz[1]??
foo.bar[0].baz[2].egg??
"
        .trim(),
        ParseOptions::from(Mode::Ipython),
    )
    .unwrap();
    insta::assert_debug_snapshot!(parsed.syntax());
}

#[test]
fn test_fstring_expr_inner_line_continuation_and_t_string() {
    let source = r#"f'{\t"i}'"#;

    let parsed = parse_expression(source);

    let error = parsed.unwrap_err();

    insta::assert_debug_snapshot!(error);
}

#[test]
fn test_fstring_expr_inner_line_continuation_newline_t_string() {
    let source = r#"f'{\
t"i}'"#;

    let parsed = parse_expression(source);

    let error = parsed.unwrap_err();

    insta::assert_debug_snapshot!(error);
}

#[test]
fn test_tstring_fstring_middle() {
    let source = "t'{:{F'{\0}F";
    let parsed = parse_expression(source);

    let error = parsed.unwrap_err();

    insta::assert_debug_snapshot!(error);
}

#[test]
fn test_tstring_fstring_middle_fuzzer() {
    let source = "A1[A\u{c}\0:+,>1t'{:f\0:{f\"f\0:\0{fm\0:{f:\u{10}\0\0\0:bb\0{@f>f\u{1}'\0f";
    let parsed = parse_expression(source);

    let error = parsed.unwrap_err();

    insta::assert_debug_snapshot!(error);
}

// --- Notebook cell boundary tests ---

use ruff_text_size::TextSize;

use crate::Parser;

/// Helper: parse with cell offsets and return (syntax errors, semantic errors).
fn parse_with_cells(source: &str, cell_offsets: &[TextSize]) -> crate::Parsed<crate::Mod> {
    let options = ParseOptions::from(Mode::Module);
    let parser = Parser::new_with_cell_offsets(source, options, cell_offsets);
    parser.parse()
}

#[test]
fn cell_boundary_if_across_cells() {
    // Cell 1: `if True:` (no body)
    // Cell 2: `    print("hello")`
    // Jupyter executes cells independently, so Cell 1 is a syntax error.
    let source = "if True:\n\n    print(\"hello\")\n";
    // Cell boundary between the two lines
    let cell_offsets = [TextSize::new(9)]; // after "if True:\n"
    let parsed = parse_with_cells(source, &cell_offsets);
    assert!(
        !parsed.errors().is_empty(),
        "Expected syntax error for `if True:` with no body in cell, got none. Errors: {:?}",
        parsed.errors()
    );
}

#[test]
fn cell_boundary_for_across_cells() {
    // Cell 1: `for x in range(10):` (no body)
    // Cell 2: `    print(x)`
    let source = "for x in range(10):\n\n    print(x)\n";
    let cell_offsets = [TextSize::new(20)]; // after "for x in range(10):\n"
    let parsed = parse_with_cells(source, &cell_offsets);
    assert!(
        !parsed.errors().is_empty(),
        "Expected syntax error for `for` with no body in cell"
    );
}

#[test]
fn cell_boundary_def_across_cells() {
    // Cell 1: `def foo():` (no body)
    // Cell 2: `    return 1`
    let source = "def foo():\n\n    return 1\n";
    let cell_offsets = [TextSize::new(11)]; // after "def foo():\n"
    let parsed = parse_with_cells(source, &cell_offsets);
    assert!(
        !parsed.errors().is_empty(),
        "Expected syntax error for `def` with no body in cell"
    );
}

#[test]
fn cell_boundary_complete_blocks_no_error() {
    // Cell 1: complete if block
    // Cell 2: standalone expression
    let source = "if True:\n    print(1)\n\nprint(2)\n";
    let cell_offsets = [TextSize::new(21)]; // after "if True:\n    print(1)\n"
    let parsed = parse_with_cells(source, &cell_offsets);
    assert!(
        parsed.errors().is_empty(),
        "Expected no syntax error for complete blocks across cells, got: {:?}",
        parsed.errors()
    );
}

#[test]
fn cell_boundary_deeply_nested_across_cells() {
    // Cell 1: `if True:\n    if True:` (two levels open)
    // Cell 2: `        print(1)\n    print(2)`
    let source = "if True:\n    if True:\n\n        print(1)\n    print(2)\n";
    let cell_offsets = [TextSize::new(24)]; // after "if True:\n    if True:\n"
    let parsed = parse_with_cells(source, &cell_offsets);
    assert!(
        !parsed.errors().is_empty(),
        "Expected syntax error for nested `if` spanning cells"
    );
}

#[test]
fn cell_boundary_while_across_cells() {
    // Cell 1: `while True:` (no body)
    // Cell 2: `    break`
    let source = "while True:\n\n    break\n";
    let cell_offsets = [TextSize::new(12)]; // after "while True:\n"
    let parsed = parse_with_cells(source, &cell_offsets);
    assert!(
        !parsed.errors().is_empty(),
        "Expected syntax error for `while` with no body in cell"
    );
}

#[test]
fn cell_boundary_class_across_cells() {
    // Cell 1: `class Foo:` (no body)
    // Cell 2: `    pass`
    let source = "class Foo:\n\n    pass\n";
    let cell_offsets = [TextSize::new(11)]; // after "class Foo:\n"
    let parsed = parse_with_cells(source, &cell_offsets);
    assert!(
        !parsed.errors().is_empty(),
        "Expected syntax error for `class` with no body in cell"
    );
}

#[test]
fn cell_boundary_with_across_cells() {
    // Cell 1: `with open("f"):` (no body)
    // Cell 2: `    pass`
    let source = "with open(\"f\"):\n\n    pass\n";
    let cell_offsets = [TextSize::new(16)]; // after "with open(\"f\"):\n"
    let parsed = parse_with_cells(source, &cell_offsets);
    assert!(
        !parsed.errors().is_empty(),
        "Expected syntax error for `with` with no body in cell"
    );
}

#[test]
fn cell_boundary_try_across_cells() {
    // Cell 1: `try:` (no body)
    // Cell 2: `    pass\nexcept:\n    pass`
    let source = "try:\n\n    pass\nexcept:\n    pass\n";
    let cell_offsets = [TextSize::new(5)]; // after "try:\n"
    let parsed = parse_with_cells(source, &cell_offsets);
    assert!(
        !parsed.errors().is_empty(),
        "Expected syntax error for `try` with no body in cell"
    );
}

#[test]
fn cell_boundary_multi_cell_complete() {
    // Three cells, all complete
    let source = "x = 1\n\ny = 2\n\nz = 3\n";
    let cell_offsets = [TextSize::new(6), TextSize::new(12)]; // after "x = 1\n" and "y = 2\n"
    let parsed = parse_with_cells(source, &cell_offsets);
    assert!(
        parsed.errors().is_empty(),
        "Expected no syntax error for complete cells, got: {:?}",
        parsed.errors()
    );
}

#[test]
fn cell_boundary_no_offsets_is_normal() {
    // Without cell offsets, `if True:\n\n    print(1)` is valid (blank line is fine)
    let source = "if True:\n\n    print(1)\n";
    let parsed = parse_with_cells(source, &[]);
    assert!(
        parsed.errors().is_empty(),
        "Expected no syntax error without cell offsets, got: {:?}",
        parsed.errors()
    );
}
