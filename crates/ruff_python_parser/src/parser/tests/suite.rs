#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;

    use crate::{lexer, parse, parse_expression, parse_suite, parse_tokens, Mode};

    #[test]
    fn test_parse_empty() {
        let parse_ast = parse_suite("").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_string() {
        let source = "'Hello world'";
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_f_string() {
        let source = "f'Hello world'";
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_print_hello() {
        let source = "print('Hello world')";
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_print_2() {
        let source = "print('Hello world', 2)";
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_kwargs() {
        let source = "my_func('positional', keyword=2)";
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_if_elif_else() {
        let source = "if 1: 10\nelif 2: 20\nelse: 30";
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_lambda() {
        let source = "lambda x, y: x * y"; // lambda(x, y): x * y";
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_lambda_no_args() {
        let source = "lambda: 1";
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_tuples() {
        let source = "a, b = 4, 5";

        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_parse_class() {
        let source = "\
class Foo(A, B):
 def __init__(self):
  pass
 def method_with_default(self, arg='default'):
  pass
";
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_parse_class_generic_types() {
        let source = "\
# TypeVar
class Foo[T](): ...

# TypeVar with bound
class Foo[T: str](): ...

# TypeVar with tuple bound
class Foo[T: (str, bytes)](): ...

# Multiple TypeVar
class Foo[T, U](): ...

# Trailing comma
class Foo[T, U,](): ...

# TypeVarTuple
class Foo[*Ts](): ...

# ParamSpec
class Foo[**P](): ...

# Mixed types
class Foo[X, Y: str, *U, **P]():
  pass
";
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }
    #[test]
    fn test_parse_function_definition() {
        let source = "\
def func(a):
    ...

def func[T](a: T) -> T:
    ...

def func[T: str](a: T) -> T:
    ...

def func[T: (str, bytes)](a: T) -> T:
    ...

def func[*Ts](*a: *Ts):
    ...

def func[**P](*args: P.args, **kwargs: P.kwargs):
    ...

def func[T, U: str, *Ts, **P]():
    pass
  ";
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_parse_dict_comprehension() {
        let source = "{x1: x2 for y in z}";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_list_comprehension() {
        let source = "[x for y in z]";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_double_list_comprehension() {
        let source = "[x for y, y2 in z for a in b if a < 5 if a > 10]";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_generator_comprehension() {
        let source = "(x for y in z)";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_named_expression_generator_comprehension() {
        let source = "(x := y + 1 for y in z)";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_if_else_generator_comprehension() {
        let source = "(x if y else y for y in z)";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_bool_op_or() {
        let source = "x or y";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_bool_op_and() {
        let source = "x and y";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_slice() {
        let source = "x[1:2:3]";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_named_expression() {
        let source = "(x := ( y * z ))";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_with_statement() {
        let source = "\
with 0: pass
with 0 as x: pass
with 0, 1: pass
with 0 as x, 1 as y: pass
with 0 if 1 else 2: pass
with 0 if 1 else 2 as x: pass
with (): pass
with () as x: pass
with (0): pass
with (0) as x: pass
with (0,): pass
with (0,) as x: pass
with (0, 1): pass
with (0, 1) as x: pass
with (*a,): pass
with (*a,) as x: pass
with (0, *a): pass
with (0, *a) as x: pass
with (a := 0): pass
with (a := 0) as x: pass
with (a := 0, b := 1): pass
with (a := 0, b := 1) as x: pass
with (0 as a): pass
with (0 as a,): pass
with (0 as a, 1 as b): pass
with (0 as a, 1 as b,): pass
";
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_parenthesized_with_statement() {
        let source = "\
with ((a), (b)): pass
with ((a), (b), c as d, (e)): pass
with (a, b): pass
with (a, b) as c: pass
with ((a, b) as c): pass
with (a as b): pass
with (a): pass
with (a := 0): pass
with (a := 0) as x: pass
with ((a)): pass
with ((a := 0)): pass
with (a as b, (a := 0)): pass
with (a, (a := 0)): pass
with (yield): pass
with (yield from a): pass
with ((yield)): pass
with ((yield from a)): pass
";
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_with_statement_invalid() {
        for source in [
            "with 0,: pass",
            "with 0 as x,: pass",
            "with *a: pass",
            "with *a as x: pass",
            "with (*a): pass",
            "with (*a) as x: pass",
            "with *a, 0 as x: pass",
            "with (*a, 0 as x): pass",
            "with 0 as x, *a: pass",
            "with (0 as x, *a): pass",
            "with (0 as x) as y: pass",
            "with (0 as x), 1: pass",
            "with ((0 as x)): pass",
            "with a := 0 as x: pass",
            "with (a := 0 as x): pass",
        ] {
            assert!(parse_suite(source).is_err());
        }
    }

    #[test]
    fn test_star_index() {
        let source = "\
array_slice = array[0, *indexes, -1]
array[0, *indexes, -1] = array_slice
array[*indexes_to_select, *indexes_to_select]
array[3:5, *indexes_to_select]
";
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_generator_expression_argument() {
        let source = r#"' '.join(
    sql
    for sql in (
        "LIMIT %d" % limit if limit else None,
        ("OFFSET %d" % offset) if offset else None,
    )
)"#;
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_try() {
        let parse_ast = parse_suite(
            r"try:
    raise ValueError(1)
except TypeError as e:
    print(f'caught {type(e)}')
except OSError as e:
    print(f'caught {type(e)}')",
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_try_star() {
        let parse_ast = parse_suite(
            r#"try:
    raise ExceptionGroup("eg",
        [ValueError(1), TypeError(2), OSError(3), OSError(4)])
except* TypeError as e:
    print(f'caught {type(e)} with nested {e.exceptions}')
except* OSError as e:
    print(f'caught {type(e)} with nested {e.exceptions}')"#,
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_dict_unpacking() {
        let parse_ast = parse_expression(r#"{"a": "b", **c, "d": "e"}"#).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_modes() {
        let source = "a[0][1][2][3][4]";

        assert!(parse(source, Mode::Expression).is_ok());
        assert!(parse(source, Mode::Module).is_ok());
    }

    #[test]
    fn test_parse_type_declaration() {
        let source = r#"
type X = int
type X = int | str
type X = int | "ForwardRefY"
type X[T] = T | list[X[T]]  # recursive
type X[T] = int
type X[T] = list[T] | set[T]
type X[T, *Ts, **P] = (T, Ts, P)
type X[T: int, *Ts, **P] = (T, Ts, P)
type X[T: (int, str), *Ts, **P] = (T, Ts, P)

# soft keyword as alias name
type type = int
type match = int
type case = int

# soft keyword as value
type foo = type
type foo = match
type foo = case

# multine definitions
type \
	X = int
type X \
	= int
type X = \
	int
type X = (
    int
)
type \
    X[T] = T
type X \
    [T] = T
type X[T] \
    = T

# simple statements
type X = int; type X = str; type X = type
class X: type X = int
"#;
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_type_as_identifier() {
        let source = r"\
type *a + b, c   # ((type * a) + b), c
type *(a + b), c   # (type * (a + b)), c
type (*a + b, c)   # type ((*(a + b)), c)
type -a * b + c   # (type - (a * b)) + c
type -(a * b) + c   # (type - (a * b)) + c
type (-a) * b + c   # (type (-(a * b))) + c
type ().a   # (type()).a
type (()).a   # (type(())).a
type ((),).a   # (type(())).a
type [a].b   # (type[a]).b
type [a,].b   # (type[(a,)]).b  (not (type[a]).b)
type [(a,)].b   # (type[(a,)]).b
type()[a:
    b]  # (type())[a: b]
if type := 1: pass
type = lambda query: query == event
print(type(12))
type(type)
a = (
	type in C
)
a = (
	type(b)
)
type (
	X = int
)
type = 1
type = x = 1
x = type = 1
lambda x: type
";
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_invalid_type() {
        assert!(parse_suite("a: type X = int").is_err());
        assert!(parse_suite("lambda: type X = int").is_err());
    }

    #[test]
    fn numeric_literals() {
        let source = r"x = 123456789
x = 123456
x = .1
x = 1.
x = 1E+1
x = 1E-1
x = 1.000_000_01
x = 123456789.123456789
x = 123456789.123456789E123456789
x = 123456789E123456789
x = 123456789J
x = 123456789.123456789J
x = 0XB1ACC
x = 0B1011
x = 0O777
x = 0.000000006
x = 10000
x = 133333
";

        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn numeric_literals_attribute_access() {
        let source = r"x = .1.is_integer()
x = 1. .imag
x = 1E+1.imag
x = 1E-1.real
x = 123456789.123456789.hex()
x = 123456789.123456789E123456789 .real
x = 123456789E123456789 .conjugate()
x = 123456789J.real
x = 123456789.123456789J.__add__(0b1011.bit_length())
x = 0XB1ACC.conjugate()
x = 0B1011 .conjugate()
x = 0O777 .real
x = 0.000000006  .hex()
x = -100.0000J

if 10 .real:
    ...

y = 100[no]
y = 100(no)
";
        assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_match_as_identifier() {
        let source = r"\
match *a + b, c   # ((match * a) + b), c
match *(a + b), c   # (match * (a + b)), c
match (*a + b, c)   # match ((*(a + b)), c)
match -a * b + c   # (match - (a * b)) + c
match -(a * b) + c   # (match - (a * b)) + c
match (-a) * b + c   # (match (-(a * b))) + c
match ().a   # (match()).a
match (()).a   # (match(())).a
match ((),).a   # (match(())).a
match [a].b   # (match[a]).b
match [a,].b   # (match[(a,)]).b  (not (match[a]).b)
match [(a,)].b   # (match[(a,)]).b
match()[a:
    b]  # (match())[a: b]
if match := 1: pass
match match:
    case 1: pass
    case 2:
        pass
match = lambda query: query == event
print(match(12))
";
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_patma() {
        let source = r#"# Cases sampled from Lib/test/test_patma.py

# case test_patma_098
match x:
    case -0j:
        y = 0
# case test_patma_142
match x:
    case bytes(z):
        y = 0
# case test_patma_073
match x:
    case 0 if 0:
        y = 0
    case 0 if 1:
        y = 1
# case test_patma_006
match 3:
    case 0 | 1 | 2 | 3:
        x = True
# case test_patma_049
match x:
    case [0, 1] | [1, 0]:
        y = 0
# case black_check_sequence_then_mapping
match x:
    case [*_]:
        return "seq"
    case {}:
        return "map"
# case test_patma_035
match x:
    case {0: [1, 2, {}]}:
        y = 0
    case {0: [1, 2, {}] | True} | {1: [[]]} | {0: [1, 2, {}]} | [] | "X" | {}:
        y = 1
    case []:
        y = 2
# case test_patma_107
match x:
    case 0.25 + 1.75j:
        y = 0
# case test_patma_097
match x:
    case -0j:
        y = 0
# case test_patma_007
match 4:
    case 0 | 1 | 2 | 3:
        x = True
# case test_patma_154
match x:
    case 0 if x:
        y = 0
# case test_patma_134
match x:
    case {1: 0}:
        y = 0
    case {0: 0}:
        y = 1
    case {**z}:
        y = 2
# case test_patma_185
match Seq():
    case [*_]:
        y = 0
# case test_patma_063
match x:
    case 1:
        y = 0
    case 1:
        y = 1
# case test_patma_248
match x:
    case {"foo": bar}:
        y = bar
# case test_patma_019
match (0, 1, 2):
    case [0, 1, *x, 2]:
        y = 0
# case test_patma_052
match x:
    case [0]:
        y = 0
    case [1, 0] if (x := x[:0]):
        y = 1
    case [1, 0]:
        y = 2
# case test_patma_191
match w:
    case [x, y, *_]:
        z = 0
# case test_patma_110
match x:
    case -0.25 - 1.75j:
        y = 0
# case test_patma_151
match (x,):
    case [y]:
        z = 0
# case test_patma_114
match x:
    case A.B.C.D:
        y = 0
# case test_patma_232
match x:
    case None:
        y = 0
# case test_patma_058
match x:
    case 0:
        y = 0
# case test_patma_233
match x:
    case False:
        y = 0
# case test_patma_078
match x:
    case []:
        y = 0
    case [""]:
        y = 1
    case "":
        y = 2
# case test_patma_156
match x:
    case z:
        y = 0
# case test_patma_189
match w:
    case [x, y, *rest]:
        z = 0
# case test_patma_042
match x:
    case (0 as z) | (1 as z) | (2 as z) if z == x % 2:
        y = 0
# case test_patma_034
match x:
    case {0: [1, 2, {}]}:
        y = 0
    case {0: [1, 2, {}] | False} | {1: [[]]} | {0: [1, 2, {}]} | [] | "X" | {}:
        y = 1
    case []:
        y = 2
# case test_patma_123
match (0, 1, 2):
    case 0, *x:
        y = 0
# case test_patma_126
match (0, 1, 2):
    case *x, 2,:
        y = 0
# case test_patma_151
match x,:
    case y,:
        z = 0
# case test_patma_152
match w, x:
    case y, z:
        v = 0
# case test_patma_153
match w := x,:
    case y as v,:
        z = 0
"#;
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_match() {
        let parse_ast = parse_suite(
            r#"
match {"test": 1}:
    case {
        **rest,
    }:
        print(rest)
match {"label": "test"}:
    case {
        "label": str() | None as label,
    }:
        print(label)
match x:
    case [0, 1,]:
        y = 0
match x:
    case (0, 1,):
        y = 0
match x:
    case (0,):
        y = 0
match x,:
    case z:
        pass
match x, y:
    case z:
        pass
match x, y,:
    case z:
        pass
"#,
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_match_pattern_fstring_literal() {
        // F-string literal is not allowed in match pattern.
        let parse_error = parse_suite(
            r#"
match x:
    case f"{y}":
        pass
"#,
        )
        .err();
        assert!(
            parse_error.is_some(),
            "expected parse error when f-string literal is used in match pattern"
        );
    }

    #[test]
    fn test_variadic_generics() {
        let parse_ast = parse_suite(
            r"
def args_to_tuple(*args: *Ts) -> Tuple[*Ts]: ...
",
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn decorator_ranges() {
        let parse_ast = parse_suite(
            r"
@my_decorator
def test():
    pass

@class_decorator
class Abcd:
    pass
"
            .trim(),
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_ipython_escape_commands() {
        let parse_ast = parse(
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
            Mode::Ipython,
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_ipython_escape_command_parse_error() {
        let source = r"
a = 1
%timeit a == 1
    "
        .trim();
        let lxr = lexer::lex(source, Mode::Ipython);
        let parse_err = parse_tokens(lxr.collect(), source, Mode::Module).unwrap_err();
        assert_eq!(
            parse_err.to_string(),
            "IPython escape commands are only allowed in `Mode::Ipython` at byte range 6..20"
                .to_string()
        );
    }

    #[test]
    fn test_fstrings() {
        let parse_ast = parse_suite(
            r#"
f"{" f"}"
f"{foo!s}"
f"{3,}"
f"{3!=4:}"
f'{3:{"}"}>10}'
f'{3:{"{"}>10}'
f"{  foo =  }"
f"{  foo =  :.3f  }"
f"{  foo =  !s  }"
f"{  1, 2  =  }"
f'{f"{3.1415=:.1f}":*^20}'

{"foo " f"bar {x + y} " "baz": 10}
match foo:
    case "one":
        pass
    case "implicitly " "concatenated":
        pass

f"\{foo}\{bar:\}"
f"\\{{foo\\}}"
f"""{
    foo:x
        y
        z
}"""
f"{ (  foo )  = }"
"#
            .trim(),
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstrings_with_unicode() {
        let parse_ast = parse_suite(
            r#"
u"foo" f"{bar}" "baz" " some"
"foo" f"{bar}" u"baz" " some"
"foo" f"{bar}" "baz" u" some"
u"foo" f"bar {baz} really" u"bar" "no"
"#
            .trim(),
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_unicode_aliases() {
        // https://github.com/RustPython/RustPython/issues/4566
        let parse_ast = parse_suite(r#"x = "\N{BACKSPACE}another cool trick""#).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }
}
