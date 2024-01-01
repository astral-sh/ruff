#[cfg(test)]
mod tests {
    use crate::{
        lexer::lex,
        parser::{ParsedFile, Parser},
        Mode, Tok,
    };
    use insta::assert_debug_snapshot;
    use itertools::Itertools;

    fn parse(src: &str) -> ParsedFile {
        let mode = Mode::Module;
        let lexer = lex(src, mode)
            .filter_ok(|(tok, _)| !matches!(tok, Tok::Comment { .. } | Tok::NonLogicalNewline));
        let parser = Parser::new(src, mode, lexer);
        parser.parse()
    }

    #[test]
    fn parse_binary_exprs() {
        assert_debug_snapshot!(parse(
            "
1 + 2
1 + 2 - 3
1 + 2 - 3 + 4
2 * 2
1 + 2 * 2
3 ** 2
3 ** 2 * 5
1 + (2 + 3)
1 << 2
1 >> 2
1 | 2
1 ^ 2
"
        ));
    }

    #[test]
    fn parse_unary_exprs() {
        assert_debug_snapshot!(parse(
            "
-1
+1
~1
-1 + 2
---1
not x
    "
        ));
    }

    #[test]
    fn parse_call_expr() {
        assert_debug_snapshot!(parse(
            "
l()
x(1, 2)
x(1, 2, x=3, y=4)
f(*l)
f(**a)
f(*a, b, **l)
f(*a, *b)
f(
    [
        [a]
        for d in f
    ],
)
f(
    {
        [a]
        for d in f
    },
)
f(
    {
        A: [a]
        for d in f
    },
)
call(
    a=1 if True else None,
    x=0,
)
"
        ));
    }

    #[test]
    fn parse_subscript_expr() {
        assert_debug_snapshot!(parse(
            "
l[0]
l[1:]
l[1:2]
l[:2]
l[:2:]
l[:2:3]
l[1:2:3]
l[:]
l[::]
l[0][0]
l[0,1]
l[0:,]
l[0:,1]
l[0:1, 2]
l[0:1:2, 3, i:i + 1]
x[a := b]
a[:, :11]
l[1,2,3]
x[~flag]
"
        ));
    }

    #[test]
    fn parse_attribute_expr() {
        assert_debug_snapshot!(parse(
            "
value.attr
value.attr()
value().attr
value().attr().foo
value.attr.foo
"
        ));
    }

    #[test]
    fn parse_parenthesized_expr() {
        assert_debug_snapshot!(parse(
            "
(l)
(l)()
(l)()()()
(a
and
b
or
c)
"
        ));
    }

    #[test]
    fn parse_bool_op_exprs() {
        assert_debug_snapshot!(parse(
            "
a and b
a and b and c
a or b
a or b or c
a and b or c
"
        ));
    }

    #[test]
    fn parse_compare_expr() {
        assert_debug_snapshot!(parse(
            "
a == b
b < a
b > a
a >= b
a <= b
a != b
a is c
a in b
a not in c
a is not b
a < b == c > d is e not in f is not g <= h >= i != j
"
        ));
    }

    #[test]
    fn parse_string_expr() {
        assert_debug_snapshot!(parse(
            r#"
'Hello World'
"😎"
'Foo' 'Bar'
(
    'A'
    'B'
    'C'
)
'''Olá, Mundo!'''
"""ABCDE"""
(
    '''aB'''
    '''cD'''
)
b'hello world'
b'bytes' b'concatenated'
"#
        ));
    }

    #[test]
    fn parse_tuple_expr() {
        assert_debug_snapshot!(parse(
            "
1, 2
1 + 2,
x and y,
(1, 2,)
(1,2,3,4)
(x + 1, l,)
()
1, 2, 3, 4
"
        ));
    }

    #[test]
    fn parse_generator_expr() {
        assert_debug_snapshot!(parse(
            "
(i for i in list)
(a async for i in iter)
(b for c in d if x in w if y and yy if z)
(a for b in c if d and e for f in j if k > h)
(a for b in c if d and e async for f in j if k > h)
f(x for i in l)
f(a, x for i in l)
f(a, x for i, j in l)
"
        ));
    }

    #[test]
    fn parse_list_expr() {
        assert_debug_snapshot!(parse(
            "
[1 + i, [1, 2, 3, 4], (a, i + x, y), {a, b, c}, {a: 1}]
[1, 2, 3]
[]
[1]
[f(g(attr.H()) for c in l)]
"
        ));
    }

    #[test]
    fn parse_list_comp_expr() {
        assert_debug_snapshot!(parse(
            "
[x for i in range(5)]
[b for c in d if x in w if y and yy if z]
[a for b in c if d and e for f in j if k > h]
[a for b in c if d and e async for f in j if k > h]
[1 for i in x in a]
[a for a, b in G]
[
    await x for a, b in C
]
[i for i in await x if entity is not None]
[x for x in (l if True else L) if T]
[i for i in (await x if True else X) if F]
[i for i in await (x if True else X) if F]
[f for f in c(x if True else [])]
"
        ));
    }

    #[test]
    fn parse_set_expr() {
        assert_debug_snapshot!(parse(
            "
{1, 2, 3}
{1 + 2, (a, b), {1,2,3}, {a:b, **d}}
{a}
"
        ));
    }

    #[test]
    fn parse_set_comp_expr() {
        assert_debug_snapshot!(parse(
            "
{x for i in ll}
{b for c in d if x in w if y and yy if z}
{a for b in c if d and e for f in j if k > h}
{a for b in c if d and e async for f in j if k > h}
{a for a, b in G}
"
        ));
    }

    #[test]
    fn parse_dict_expr() {
        assert_debug_snapshot!(parse(
            "
{}
{1:2, a:1, b:'hello'}
{a:b, **d}
{'foo': 'bar', **{'nested': 'dict'}}
{x + 1: y * 2, **call()}
{l: [1, 2, 3], t: (1,2,3), d: {1:2, 3:4}, s: {1, 2}}
{**d}
{1: 2, **{'nested': 'dict'}}
{a: c}
{i: tuple(j for j in t if i != j)
           for t in L
           for i in t}
{
    'A': lambda p: None,
    'B': C,
}
{**a, **b}
"
        ));
    }

    #[test]
    fn parse_dict_comp_expr() {
        assert_debug_snapshot!(parse(
            "
{1: 2 for i in a}
{x + 1: 'x' for i in range(5)}
{b: c * 2 for c in d if x in w if y and yy if z}
{a: a ** 2 for b in c if d and e for f in j if k > h}
{a: b for b in c if d and e async for f in j if k > h}
{a: a for b, c in d}
"
        ));
    }

    #[test]
    fn parse_starred_expr() {
        assert_debug_snapshot!(parse(
            "
*a
*(a + 1)
*x.attr
"
        ));
    }

    #[test]
    fn parse_await_expr() {
        assert_debug_snapshot!(parse(
            "
await x
await x + 1
await a and b
await f()
await [1, 2]
await {3, 4}
await {i: 5}
await 7, 8
await (9, 10)
await 1 == 1
await x if True else None
"
        ));
    }

    #[test]
    fn parse_yield_expr() {
        assert_debug_snapshot!(parse(
            "
yield *y
yield x
yield x + 1
yield a and b
yield f()
yield [1, 2]
yield {3, 4}
yield {i: 5}
yield 7, 8
yield (9, 10)
yield 1 == 1
"
        ));
    }

    #[test]
    fn parse_yield_from_expr() {
        assert_debug_snapshot!(parse(
            "
yield from x
yield from x + 1
yield from a and b
yield from f()
yield from [1, 2]
yield from {3, 4}
yield from {i: 5}
yield from (9, 10)
yield from 1 == 1
"
        ));
    }

    #[test]
    fn parse_if_else_expr() {
        assert_debug_snapshot!(parse(
            "
a if True else b
f() if x else None
a if b else c if d else e
1 + x if 1 < 0 else -1
a and b if x else False
x <= y if y else x
True if a and b else False
1, 1 if a else c
"
        ));
    }

    #[test]
    fn parse_lambda_expr() {
        assert_debug_snapshot!(parse(
            "
lambda: a
lambda x: 1
lambda x, y: ...
lambda y, z=1: z * y
lambda *a: a
lambda *a, z, x=0: ...
lambda **kwargs: f()
lambda *args, **kwargs: f() + 1
lambda *args, a, b=1, **kwargs: f() + 1
lambda a, /: ...
lambda a, /, b: ...
lambda a=1, /,: ...
lambda a, b, /, *, c: ...
lambda kw=1, *, a: ...
"
        ));
    }

    #[test]
    fn parse_named_expr() {
        assert_debug_snapshot!(parse(
            "
(x:=1)
{ x := 1 }
[x := 1]
(x := 1 + 1)
(x,y := a and b)
{ x,y := a < b }
[x,y := ...]
f(a:=b, c:=d)
"
        ));
    }

    #[test]
    fn parse_if_stmt() {
        assert_debug_snapshot!(parse(
            "
if True:
    1
    ...
if x < 1:
    ...
else:
    pass

if a:
    pass
elif b:
    ...

if a and b:
    ...
elif True:
    ...
elif c:
    ...
elif d:
    ...
else:
    f()
if a:=b: ...
"
        ));
    }

    #[test]
    fn parse_simple_stmts() {
        assert_debug_snapshot!(parse(
            "
if x: ...
if True: pass
1; 2; pass
1; ...; a if b else c

continue

break

del a
del a, b, 1, 1 + 2,
del a, (b, c), d

assert 1 < 2
assert f()
assert a and b
assert x, 'error'

global a
global a, b, c

return
return a and b
return 1 < 2
return None
return 1, 2,
return x
return f()
return a.f()

nonlocal a
nonlocal a, b, c

raise
raise a
raise 1 < 2
raise a and b
raise a from b

import a
import a.b.c
import a.b.c as d
import a, b, c
import foo.bar as a, a.b.c.d as abcd

from a import b # comment
from . import a
from foo.bar import baz as b, FooBar as fb
from .a import b
from ... import c
from .......................... import d
from ..........................a.b.c import d
from module import (a, b as B, c,)
from a import *

if c: B; del A
else: C
if x: yield x;
"
        ));
    }

    #[test]
    fn parse_func_def_stmt() {
        assert_debug_snapshot!(parse(
            "
def f():
    ...
def x() -> int:
    f()
    pass
    ...
def mul(x, y) -> 'str':
    x * y
def f1(*a): ...
def f2(*a, z, x=0): ...
def f3(**kwargs): f()
def f4(*args, **kwargs): f() + 1
def f5(*args, a, b=1, **kwargs): f() + 1
def f6(a, /): ...
def f7(a, /, b): ...
def f8(a=1, /,): ...
def f9(a, b, /, *, c): ...
def f10(kw=1, *, a): ...
def f11(x: int, y: 'str', z: 1 + 2): pass
def f12(self, a=1, b=2, c=3): ...
"
        ));
    }

    #[test]
    fn parse_class_def_stmt() {
        assert_debug_snapshot!(parse(
            "
class T:
    ...
class Test():
        def __init__(self):
            pass
class T(a=1, *A, **k):
    ...
class T:
    def f():
        a, b = l
"
        ));
    }

    #[test]
    fn parse_decorators() {
        assert_debug_snapshot!(parse(
            "
@a
def f(): ...

@a.b.c
def f(): ...

@a
@a.b.c
def f(): ...

@a
@1 | 2
@a.b.c
class T: ...

@named_expr := abc
def f():
    ...
"
        ));
    }

    #[test]
    fn parse_with_stmt() {
        assert_debug_snapshot!(parse(
            "
with x:
    ...
with x, y:
    ...
with open() as f:
    ...
with f() as x.attr:
    pass
with x as X, y as Y, z as Z:
    ...
with (x, z as Y, y,):
    ...
with (a) as f:
    ...
with ((a) as f, 1):
    ...
with a:
    yield a, b
with (yield 1):
    ...
with (yield from 1):
    ...
with (a := 1):
    ...
with (open('bla.txt')), (open('bla.txt')):
    pass
with (a := 1, x):
    ...
with (p / 'new_file').open('wb'): ...
"
        ));
    }

    #[test]
    fn parse_while_stmt() {
        assert_debug_snapshot!(parse(
            "
while x:
    ...
while (x > 1) and y:
    pass
else:
    ...
while x and y:
    ...
    print('Hello World!')

else:
    print('Olá, Mundo!')
    ...
while a := b: ...
"
        ));
    }

    #[test]
    fn parse_for_stmt() {
        assert_debug_snapshot!(parse(
            "
for i in x:
    ...
for x.attr in f():
    pass
for 1 + 2 in x.attr:
    ...
for i in x <= y:
    pass
for i in a and b:
    pass
for a,b,c, in iter:
    ...
for (a, b) in iter:
    ...
for i in *x.attr:
    ...
for -i in [1, 2]:
    ...
for *l in a, b, c,:
   ...
else:
    pass
"
        ));
    }

    #[test]
    fn parse_try_stmt() {
        assert_debug_snapshot!(parse(
            "
try:
    ...
except:
    ...

try:
    ...
except Exception1 as e:
    ...
except Exception2 as e:
    ...

try:
    ...
except Exception as e:
    ...
except:
    ...
finally:
    ...

try:
    ...
except:
    ...
else:
    ...

try:
    ...
except:
    ...
else:
    ...
finally:
    ...

try:
    ...
finally:
    ...

try:
    ...
else:
    ...
finally:
    ...

try:
    ...
except* a as A:
    ...
except* b:
    ...
"
        ));
    }

    #[test]
    fn parse_async_stmt() {
        assert_debug_snapshot!(parse(
            "
async def f():
    ...

async for i in iter:
    ...

async with x:
    ...

@a
async def x():
    ...
"
        ));
    }

    #[test]
    fn parse_assign_stmt() {
        assert_debug_snapshot!(parse(
            "
x = 1
[] = *l
() = *t
a, b = ab
*a = 1 + 2
a = b = c
foo.bar = False
baz[0] = 42
"
        ));
    }

    #[test]
    fn parse_ann_assign_stmt() {
        assert_debug_snapshot!(parse(
            "
x: int
(y): 1 + 2
var: tuple[int] | int = 1,
"
        ));
    }

    #[test]
    fn parse_aug_assign_stmt() {
        assert_debug_snapshot!(parse(
            "
a += 1
a *= b
a -= 1
a /= a + 1
a //= (a + b) - c ** 2
a @= [1,2]
a %= x
a |= 1
a <<= 2
a >>= 2
a ^= ...
a **= 42
"
        ));
    }

    #[test]
    fn parse_match_stmt() {
        assert_debug_snapshot!(parse(
            "
# PatternMatchSingleton
match x:
    case None:
        ...
    case True:
        ...
    case False:
        ...

# PatternMatchValue
match x:
    case a.b:
        ...
    case a.b.c:
        ...
    case '':
        ...
    case b'':
        ...
    case 1:
        ...
    case 1.0:
        ...
    case 1.0J:
        ...
    case 1 + 1:
        ...
    case -1:
        ...
    case -1.:
        ...
    case -0b01:
        ...
    case (1):
        ...

# PatternMatchOr
match x:
    case 1 | 2:
        ...
    case '' | 1.1 | -1 | 1 + 1 | a.b:
        ...

# PatternMatchAs
match x:
    case a as b:
        ...
    case 1 | 2 as two:
        ...
    case 1 + 3 as sum:
        ...
    case a.b as ab:
        ...
    case _:
        ...
    case _ as x:
        ...

# PatternMatchSequence
match x:
    case 1, 2, 3:
        ...
    case (1, 2, 3,):
        ...
    case (1 + 2, a, None, a.b):
        ...
    case (1 as X, b) as S:
        ...
    case [1, 2, 3 + 1]:
        ...
    case ([1,2], 3):
        ...
    case [1]:
        ...

# PatternMatchStar
match x:
    case *a:
        ...
    case *_:
        ...
    case [1, 2, *rest]:
        ...
    case (*_, 1, 2):
        ...

# PatternMatchClass
match x:
    case Point():
        ...
    case a.b.Point():
        ...
    case Point2D(x=0):
        ...
    case Point2D(x=0, y=0,):
        ...
    case Point2D(0, 1):
        ...
    case Point2D([0, 1], y=1):
        ...
    case Point2D(x=[0, 1], y=1):
        ...

# PatternMatchMapping
match x := b:
    case {1: _}:
        ...
    case {'': a, None: (1, 2), **rest}:
        ...

# Pattern guard
match y:
    case a if b := c: ...
    case e if  1 < 2: ...
"
        ));
    }

    #[test]
    fn parse_type_alias_stmt() {
        assert_debug_snapshot!(parse(
            "
type Point = tuple[float, float]
type Point[T] = tuple[T, T]
type IntFunc[**P] = Callable[P, int]  # ParamSpec
type LabeledTuple[*Ts] = tuple[str, *Ts]  # TypeVarTuple
type HashableSequence[T: Hashable] = Sequence[T]  # TypeVar with bound
type IntOrStrSequence[T: (int, str)] = Sequence[T]  # TypeVar with constraints
"
        ));
    }

    #[test]
    fn parse_type_params() {
        assert_debug_snapshot!(parse(
            "
def max[T](args: Iterable[T]) -> T:
    ...
class list[T]:
    ...
"
        ));
    }

    #[test]
    fn parse_empty_fstring() {
        assert_debug_snapshot!(parse(
            r#"
f""
F""
f''
f""""""
f''''''
"#
        ));
    }

    #[test]
    fn parse_fstring() {
        assert_debug_snapshot!(parse(
            r#"
f"normal {foo} {{another}} {bar} {{{three}}}"
f"normal {foo!a} {bar!s} {baz!r} {foobar}"
f"normal {x:y + 2}"
f"{x:{{1}.pop()}}"
f"{(lambda x:{x})}"
f"{x =}"
f"{    x = }"
f"{x=!a}"
f"{x:.3f!r =}"
f"{x = !r :.3f}"
f"{x:.3f=!r}"
"hello" f"{x}"
f"{x}" f"{y}"
f"{x}" "world"
f"Invalid args in command: {command, *args}"
"foo" f"{x}" "bar"
(
    f"a"
    F"b"
    "c"
    rf"d"
    fr"e"
)
"#
        ));
    }
}
