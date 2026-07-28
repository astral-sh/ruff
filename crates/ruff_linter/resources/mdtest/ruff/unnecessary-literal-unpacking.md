# `unnecessary-literal-unpacking` (`RUF077`)

```toml
[lint]
preview = true
select = ["RUF077"]
```

## Call arguments

```py
def foo(*args, **kwargs): ...


bar = 1
baz = 2
rest = [3, 4]

foo(*[bar])  # snapshot: unnecessary-literal-unpacking
foo(*(bar,))  # error: [unnecessary-literal-unpacking]
foo(*[bar, *rest])  # error: [unnecessary-literal-unpacking]
foo(*(bar, *rest))  # error: [unnecessary-literal-unpacking]
foo(*[*rest, bar])  # error: [unnecessary-literal-unpacking]
foo(*(*rest, bar))  # error: [unnecessary-literal-unpacking]
foo(*[bar], baz)  # error: [unnecessary-literal-unpacking]
foo(baz, *[bar])  # error: [unnecessary-literal-unpacking]
foo(*[bar], *rest)  # error: [unnecessary-literal-unpacking]
foo(*[bar], keyword=baz)  # error: [unnecessary-literal-unpacking]
foo(*[bar], **{"keyword": baz})  # error: [unnecessary-literal-unpacking]
foo(*[bar,])  # error: [unnecessary-literal-unpacking]
foo(* [bar])  # error: [unnecessary-literal-unpacking]
# error: [unnecessary-literal-unpacking]
foo(*
    [bar])
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
 --> src/mdtest_snippet.py:8:5
  |
8 | foo(*[bar])  # snapshot: unnecessary-literal-unpacking
  |     ^^^^^^
  |
help: Remove unnecessary list
  |
7 |
  - foo(*[bar])  # snapshot: unnecessary-literal-unpacking
8 + foo(bar)  # snapshot: unnecessary-literal-unpacking
9 | foo(*(bar,))  # error: [unnecessary-literal-unpacking]
  |
```

## Collection displays

```py
bar = 1
baz = 2
rest = [3, 4]

[*[bar, baz], *rest]  # snapshot: unnecessary-literal-unpacking
(*[bar, baz], *rest)  # error: [unnecessary-literal-unpacking]
{*[bar, baz], *rest}  # error: [unnecessary-literal-unpacking]
[*(bar,)]  # error: [unnecessary-literal-unpacking]
[bar, *[baz]]  # error: [unnecessary-literal-unpacking]
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
 --> src/mdtest_snippet.py:5:2
  |
5 | [*[bar, baz], *rest]  # snapshot: unnecessary-literal-unpacking
  |  ^^^^^^^^^^^
  |
help: Remove unnecessary list
  |
4 |
  - [*[bar, baz], *rest]  # snapshot: unnecessary-literal-unpacking
5 + [bar, baz, *rest]  # snapshot: unnecessary-literal-unpacking
6 | (*[bar, baz], *rest)  # error: [unnecessary-literal-unpacking]
  |
```

## Bare tuples

A tuple written without parentheses keeps the comma that makes it a tuple, so its elements can be
expanded like any others.

```py
bar = 1
baz = 2

values = *[bar],  # snapshot: unnecessary-literal-unpacking
values = *[bar], baz  # error: [unnecessary-literal-unpacking]
values = *(bar, baz),  # error: [unnecessary-literal-unpacking]


def f():
    return *[bar], baz  # error: [unnecessary-literal-unpacking]


for value in *[bar], baz:  # error: [unnecessary-literal-unpacking]
    pass
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
 --> src/mdtest_snippet.py:4:10
  |
4 | values = *[bar],  # snapshot: unnecessary-literal-unpacking
  |          ^^^^^^
  |
help: Remove unnecessary list
  |
3 |
  - values = *[bar],  # snapshot: unnecessary-literal-unpacking
4 + values = bar,  # snapshot: unnecessary-literal-unpacking
5 | values = *[bar], baz  # error: [unnecessary-literal-unpacking]
  |
```

## Redundant parentheses

Parentheses between the `*` and the literal go along with the brackets. Leaving them behind would
turn the expanded elements back into a single tuple argument.

```py
foo = print
bar = 1
baz = 2

foo(*([bar, baz]))  # snapshot: unnecessary-literal-unpacking
foo(*((bar, baz)))  # error: [unnecessary-literal-unpacking]
foo(*(((bar, baz))))  # error: [unnecessary-literal-unpacking]
foo(*( [bar, baz] ))  # error: [unnecessary-literal-unpacking]
foo(*([bar]))  # error: [unnecessary-literal-unpacking]
foo(*([]))  # error: [unnecessary-literal-unpacking]
[*([bar, baz]), bar]  # error: [unnecessary-literal-unpacking]
values = *([bar]),  # error: [unnecessary-literal-unpacking]
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
 --> src/mdtest_snippet.py:5:5
  |
5 | foo(*([bar, baz]))  # snapshot: unnecessary-literal-unpacking
  |     ^^^^^^^^^^^^^
  |
help: Remove unnecessary list
  |
4 |
  - foo(*([bar, baz]))  # snapshot: unnecessary-literal-unpacking
5 + foo(bar, baz)  # snapshot: unnecessary-literal-unpacking
6 | foo(*((bar, baz)))  # error: [unnecessary-literal-unpacking]
  |
```

A comment after the literal is left where it is, since only the parentheses themselves go:

```py
foo = print
bar = 1

# snapshot: unnecessary-literal-unpacking
foo(*([bar]  # comment
))
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:17:5
   |
17 |   foo(*([bar]  # comment
   |  _____^
18 | | ))
   | |_^
   |
help: Remove unnecessary list
   |
16 | # snapshot: unnecessary-literal-unpacking
   - foo(*([bar]  # comment
   - ))
17 + foo(bar  # comment
18 + )
   |
```

## Class bases

The base list of a class definition is not wrapped in an expression, so the elements are expanded
into the statement's own argument list.

```py
class Base: ...


class Meta(type): ...


class C1(*[Base]): ...  # snapshot: unnecessary-literal-unpacking


class C2(*[Base], metaclass=Meta): ...  # error: [unnecessary-literal-unpacking]


class C3(*[]): ...  # snapshot: unnecessary-literal-unpacking
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
 --> src/mdtest_snippet.py:7:10
  |
7 | class C1(*[Base]): ...  # snapshot: unnecessary-literal-unpacking
  |          ^^^^^^^
  |
help: Remove unnecessary list
  |
6 |
  - class C1(*[Base]): ...  # snapshot: unnecessary-literal-unpacking
7 + class C1(Base): ...  # snapshot: unnecessary-literal-unpacking
8 |
  |


error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:13:10
   |
13 | class C3(*[]): ...  # snapshot: unnecessary-literal-unpacking
   |          ^^^
   |
help: Remove unnecessary list
   |
12 |
   - class C3(*[]): ...  # snapshot: unnecessary-literal-unpacking
13 + class C3: ...  # snapshot: unnecessary-literal-unpacking
   |
```

## Nested unpacking

Each level is reported separately, so repeated fix passes unwrap the whole nest.

```py
bar = 1

foo = print
# error: [unnecessary-literal-unpacking]
# error: [unnecessary-literal-unpacking]
foo(*[*[bar]])
```

## Set literals

A single-element set literal can never do anything, so unpacking one is reported. The fix is unsafe:
building the set requires the element to be hashable, and dropping that check would silence a
`TypeError`.

```py
foo = print
bar = 1
baz = 2
rest = [3, 4]

foo(*{bar})  # snapshot: unnecessary-literal-unpacking
foo(*{bar}, baz)  # error: [unnecessary-literal-unpacking]
[*{bar}, baz]  # error: [unnecessary-literal-unpacking]
{*{bar}, baz}  # error: [unnecessary-literal-unpacking]
values = *{bar},  # error: [unnecessary-literal-unpacking]


class C(*{bar}): ...  # error: [unnecessary-literal-unpacking]
```

```snapshot
error[RUF077]: Unnecessary unpacking of set literal
 --> src/mdtest_snippet.py:6:5
  |
6 | foo(*{bar})  # snapshot: unnecessary-literal-unpacking
  |     ^^^^^^
  |
help: Remove unnecessary set
  |
5 |
  - foo(*{bar})  # snapshot: unnecessary-literal-unpacking
6 + foo(bar)  # snapshot: unnecessary-literal-unpacking
7 | foo(*{bar}, baz)  # error: [unnecessary-literal-unpacking]
  |
note: This is an unsafe fix and may change runtime behavior
```

Unpacking any other set literal is left alone, because the set is doing real work — it
deduplicates:

```py
foo = print
bar = 1
baz = 2
rest = [3, 4]

foo(*{bar, baz})
foo(*{*rest})
foo(*{bar, *rest})
[*{bar, baz}, bar]
values = *{bar, baz},
```

A single-element set nested inside a list is still expanded, and the set itself is still reported:

```py
foo = print
bar = 1

# error: [unnecessary-literal-unpacking]
# error: [unnecessary-literal-unpacking]
foo(*[*{bar}])
```

## Comments inside the literal

Only the `*` and the brackets are removed, so comments survive.

```py
foo = print
bar = 1
baz = 2

foo(
    # snapshot: unnecessary-literal-unpacking
    *[
        # leading comment
        bar,  # trailing comment
        baz,
    ]
)
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:7:5
   |
 7 | /     *[
 8 | |         # leading comment
 9 | |         bar,  # trailing comment
10 | |         baz,
11 | |     ]
   | |_____^
   |
help: Remove unnecessary list
   |
6  |     # snapshot: unnecessary-literal-unpacking
   -     *[
7  +     
8  |         # leading comment
9  |         bar,  # trailing comment
   -         baz,
   -     ]
10 +         baz
11 +     
12 | )
   |
```

## Empty literals

```py
def foo(*args): ...


bar = 1
baz = 2

foo(*[])  # error: [unnecessary-literal-unpacking]
foo(*(), bar)  # snapshot: unnecessary-literal-unpacking
foo(bar, *[])  # error: [unnecessary-literal-unpacking]
[bar, *[]]  # error: [unnecessary-literal-unpacking]
{bar, *[]}  # error: [unnecessary-literal-unpacking]
(*[], bar, baz)  # error: [unnecessary-literal-unpacking]
```

```snapshot
error[RUF077]: Unnecessary unpacking of tuple literal
 --> src/mdtest_snippet.py:8:5
  |
8 | foo(*(), bar)  # snapshot: unnecessary-literal-unpacking
  |     ^^^
  |
help: Remove unnecessary tuple
  |
7 | foo(*[])  # error: [unnecessary-literal-unpacking]
  - foo(*(), bar)  # snapshot: unnecessary-literal-unpacking
8 + foo(bar)  # snapshot: unnecessary-literal-unpacking
9 | foo(bar, *[])  # error: [unnecessary-literal-unpacking]
  |
```

A tuple shrinking to a single element gets its trailing comma back, so `(*[], bar)` becomes
`(bar,)` rather than `(bar)`, which is just `bar`. A comma that is not swallowed by the removal is reused
instead of a second one being added:

```py
bar = 1

(*[], bar)  # snapshot: unnecessary-literal-unpacking
(bar, *[])  # error: [unnecessary-literal-unpacking]
(*[], bar,)  # error: [unnecessary-literal-unpacking]
(bar, *[],)  # snapshot: unnecessary-literal-unpacking
values = *[], bar  # error: [unnecessary-literal-unpacking]
values = bar, *[]  # error: [unnecessary-literal-unpacking]
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:15:2
   |
15 | (*[], bar)  # snapshot: unnecessary-literal-unpacking
   |  ^^^
   |
help: Remove unnecessary list
   |
14 |
   - (*[], bar)  # snapshot: unnecessary-literal-unpacking
15 + (bar,)  # snapshot: unnecessary-literal-unpacking
16 | (bar, *[])  # error: [unnecessary-literal-unpacking]
   |


error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:18:7
   |
18 | (bar, *[],)  # snapshot: unnecessary-literal-unpacking
   |       ^^^
   |
help: Remove unnecessary list
   |
17 | (*[], bar,)  # error: [unnecessary-literal-unpacking]
   - (bar, *[],)  # snapshot: unnecessary-literal-unpacking
18 + (bar,)  # snapshot: unnecessary-literal-unpacking
19 | values = *[], bar  # error: [unnecessary-literal-unpacking]
   |
```

A display losing its only element is rewritten wholesale rather than emptied element by element: a
trailing comma written after that element would be left behind as `[,]`, deleting the unpacking
alone would leave `(,)`, and a set display cannot shrink to `{}`, which is an empty dict:

```py
{*[]}  # snapshot: unnecessary-literal-unpacking
(*[],)  # snapshot: unnecessary-literal-unpacking
values = *[],  # snapshot: unnecessary-literal-unpacking
values = (*(),)  # error: [unnecessary-literal-unpacking]
[*[]]  # error: [unnecessary-literal-unpacking]
[*[],]  # snapshot: unnecessary-literal-unpacking
values = [
    *[],  # error: [unnecessary-literal-unpacking]
]
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:21:2
   |
21 | {*[]}  # snapshot: unnecessary-literal-unpacking
   |  ^^^
   |
help: Remove unnecessary list
   |
20 | values = bar, *[]  # error: [unnecessary-literal-unpacking]
   - {*[]}  # snapshot: unnecessary-literal-unpacking
21 + set()  # snapshot: unnecessary-literal-unpacking
22 | (*[],)  # snapshot: unnecessary-literal-unpacking
   |


error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:22:2
   |
22 | (*[],)  # snapshot: unnecessary-literal-unpacking
   |  ^^^
   |
help: Remove unnecessary list
   |
21 | {*[]}  # snapshot: unnecessary-literal-unpacking
   - (*[],)  # snapshot: unnecessary-literal-unpacking
22 + ()  # snapshot: unnecessary-literal-unpacking
23 | values = *[],  # snapshot: unnecessary-literal-unpacking
   |


error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:23:10
   |
23 | values = *[],  # snapshot: unnecessary-literal-unpacking
   |          ^^^
   |
help: Remove unnecessary list
   |
22 | (*[],)  # snapshot: unnecessary-literal-unpacking
   - values = *[],  # snapshot: unnecessary-literal-unpacking
23 + values = ()  # snapshot: unnecessary-literal-unpacking
24 | values = (*(),)  # error: [unnecessary-literal-unpacking]
   |


error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:26:2
   |
26 | [*[],]  # snapshot: unnecessary-literal-unpacking
   |  ^^^
   |
help: Remove unnecessary list
   |
25 | [*[]]  # error: [unnecessary-literal-unpacking]
   - [*[],]  # snapshot: unnecessary-literal-unpacking
26 + []  # snapshot: unnecessary-literal-unpacking
27 | values = [
   |
```

A comment anywhere inside such a display has nowhere to go, since the whole display is rewritten, so
the fix that drops it is unsafe:

```py
# snapshot: unnecessary-literal-unpacking
values = (*[],  # comment
)
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:31:11
   |
31 | values = (*[],  # comment
   |           ^^^
   |
help: Remove unnecessary list
   |
30 | # snapshot: unnecessary-literal-unpacking
   - values = (*[],  # comment
   - )
31 + values = ()
32 | def foo(*args): ...
   |
note: This is an unsafe fix and may change runtime behavior
```

A comment that only borders the deleted text does survive, because the removal stops where the
comment begins:

```py
def foo(*args): ...


bar = 1

# snapshot: unnecessary-literal-unpacking
foo(*[],  # comment
    bar)
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:39:5
   |
39 | foo(*[],  # comment
   |     ^^^
   |
help: Remove unnecessary list
   |
38 | # snapshot: unnecessary-literal-unpacking
   - foo(*[],  # comment
39 + foo(# comment
40 |     bar)
   |
```

A set losing its only element is rewritten as `set()` even where that leaves the `*` in front of it
with nothing left to expand. The outer unpacking is not itself reported: a set built from a spread
still deduplicates, so it is not a pointless literal.

```py
def foo(*args): ...


foo(*{*[]})  # snapshot: unnecessary-literal-unpacking
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:44:7
   |
44 | foo(*{*[]})  # snapshot: unnecessary-literal-unpacking
   |       ^^^
   |
help: Remove unnecessary list
   |
43 |
   - foo(*{*[]})  # snapshot: unnecessary-literal-unpacking
44 + foo(*set())  # snapshot: unnecessary-literal-unpacking
45 | set = list
   |
```

Writing `set()` needs `set` to be the builtin:

```py
set = list

{*[]}  # snapshot: unnecessary-literal-unpacking
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:47:2
   |
47 | {*[]}  # snapshot: unnecessary-literal-unpacking
   |  ^^^
   |
help: Remove unnecessary list
```

Neither shape that leaves a literal with elements unfixable applies to an empty one: the whole
unpacking goes, so no argument moves past a keyword argument and no line is left continuing into the
next.

```py
def with_keyword(*args, **kwargs): ...


qux = 3

# snapshot: unnecessary-literal-unpacking
with_keyword(keyword=qux, *[])
# snapshot: unnecessary-literal-unpacking
values = *[
], qux
# error: [unnecessary-literal-unpacking]
values = qux, *[
]
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:54:27
   |
54 | with_keyword(keyword=qux, *[])
   |                           ^^^
   |
help: Remove unnecessary list
   |
53 | # snapshot: unnecessary-literal-unpacking
   - with_keyword(keyword=qux, *[])
54 + with_keyword(keyword=qux)
55 | # snapshot: unnecessary-literal-unpacking
   |


error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:56:10
   |
56 |   values = *[
   |  __________^
57 | | ], qux
   | |_^
   |
help: Remove unnecessary list
   |
55 | # snapshot: unnecessary-literal-unpacking
   - values = *[
   - ], qux
56 + values = qux,
57 | # error: [unnecessary-literal-unpacking]
   |
```

Several empty unpackings can live in the same display. Each fix is isolated, so the fixer applies
at most one of them per pass and every fix only has to keep the display well-formed after losing a
single element. Without that, `(*[], bar, *[])` — which is the one-element tuple `(bar,)` — would
lose both unpackings at once and collapse to plain `bar`:

```py
bar = 1
baz = 2

# snapshot: unnecessary-literal-unpacking
# snapshot: unnecessary-literal-unpacking
(*[], bar, *[])
# error: [unnecessary-literal-unpacking]
# error: [unnecessary-literal-unpacking]
values = *[], bar, *[]
# error: [unnecessary-literal-unpacking]
# error: [unnecessary-literal-unpacking]
{*[], bar, *[]}
# error: [unnecessary-literal-unpacking]
# error: [unnecessary-literal-unpacking]
{*[], *[]}
# error: [unnecessary-literal-unpacking]
# error: [unnecessary-literal-unpacking]
(*[], bar, baz, *[])
# error: [unnecessary-literal-unpacking]
# error: [unnecessary-literal-unpacking]
[*[], *[]]
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:66:2
   |
66 | (*[], bar, *[])
   |  ^^^
   |
help: Remove unnecessary list
   |
65 | # snapshot: unnecessary-literal-unpacking
   - (*[], bar, *[])
66 + (bar, *[])
67 | # error: [unnecessary-literal-unpacking]
   |


error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:66:12
   |
66 | (*[], bar, *[])
   |            ^^^
   |
help: Remove unnecessary list
   |
65 | # snapshot: unnecessary-literal-unpacking
   - (*[], bar, *[])
66 + (*[], bar)
67 | # error: [unnecessary-literal-unpacking]
   |
```

An empty unpacking can also sit next to one that does have elements. The empty one has to survive
being counted as a single element even though its neighbour will expand:

```py
bar = 1
baz = 2

# error: [unnecessary-literal-unpacking]
# error: [unnecessary-literal-unpacking]
(*[], *[bar], baz)
# error: [unnecessary-literal-unpacking]
# error: [unnecessary-literal-unpacking]
(*[], *[bar, baz])
# snapshot: unnecessary-literal-unpacking
# snapshot: unnecessary-literal-unpacking
(*[], *[bar])
# error: [unnecessary-literal-unpacking]
# error: [unnecessary-literal-unpacking]
{*[], *{bar}}
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:93:2
   |
93 | (*[], *[bar])
   |  ^^^
   |
help: Remove unnecessary list
   |
92 | # snapshot: unnecessary-literal-unpacking
   - (*[], *[bar])
93 + (*[bar],)
94 | # error: [unnecessary-literal-unpacking]
   |


error[RUF077]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:93:7
   |
93 | (*[], *[bar])
   |       ^^^^^^
   |
help: Remove unnecessary list
   |
92 | # snapshot: unnecessary-literal-unpacking
   - (*[], *[bar])
93 + (*[], bar)
94 | # error: [unnecessary-literal-unpacking]
   |
```

## Subscripts

```toml
target-version = "py311"

[lint]
preview = true
select = ["RUF077"]
```

`A[*Ts]` subscripts `A` with a one-element tuple even though it is written without a comma, so the
fix has to put that comma back.

```py
from typing import Generic, TypeVarTuple

Ts = TypeVarTuple("Ts")


class A(Generic[*Ts]): ...


x: A[*(int,)]  # snapshot: unnecessary-literal-unpacking
y: A[*(int, str)]  # snapshot: unnecessary-literal-unpacking
z: A[*(int,), str]  # error: [unnecessary-literal-unpacking]
w: A[*()]  # snapshot: unnecessary-literal-unpacking
# A slice that keeps fewer than two elements is no longer a tuple, so `A[*(), int]` cannot become
# `A[int]`.
v: A[*(), int]  # snapshot: unnecessary-literal-unpacking
u: A[*(), int, str]  # error: [unnecessary-literal-unpacking]
# Redundant parentheses and the comma a one-element slice needs, at the same time.
t: A[*((int,))]  # snapshot: unnecessary-literal-unpacking
s: A[*((int, str))]  # error: [unnecessary-literal-unpacking]
# The slice is a tuple written without parentheses, but the subscript's own brackets are what
# continue the lines, so the literal's brackets can still go.
# snapshot: unnecessary-literal-unpacking
r: A[*(
    int,
), str]
```

```snapshot
error[RUF077]: Unnecessary unpacking of tuple literal
 --> src/mdtest_snippet.py:9:6
  |
9 | x: A[*(int,)]  # snapshot: unnecessary-literal-unpacking
  |      ^^^^^^^
  |
help: Remove unnecessary tuple
   |
8  |
   - x: A[*(int,)]  # snapshot: unnecessary-literal-unpacking
9  + x: A[int,]  # snapshot: unnecessary-literal-unpacking
10 | y: A[*(int, str)]  # snapshot: unnecessary-literal-unpacking
   |


error[RUF077]: Unnecessary unpacking of tuple literal
  --> src/mdtest_snippet.py:10:6
   |
10 | y: A[*(int, str)]  # snapshot: unnecessary-literal-unpacking
   |      ^^^^^^^^^^^
   |
help: Remove unnecessary tuple
   |
9  | x: A[*(int,)]  # snapshot: unnecessary-literal-unpacking
   - y: A[*(int, str)]  # snapshot: unnecessary-literal-unpacking
10 + y: A[int, str]  # snapshot: unnecessary-literal-unpacking
11 | z: A[*(int,), str]  # error: [unnecessary-literal-unpacking]
   |


error[RUF077]: Unnecessary unpacking of tuple literal
  --> src/mdtest_snippet.py:12:6
   |
12 | w: A[*()]  # snapshot: unnecessary-literal-unpacking
   |      ^^^
   |
help: Remove unnecessary tuple
   |
11 | z: A[*(int,), str]  # error: [unnecessary-literal-unpacking]
   - w: A[*()]  # snapshot: unnecessary-literal-unpacking
12 + w: A[()]  # snapshot: unnecessary-literal-unpacking
13 | # A slice that keeps fewer than two elements is no longer a tuple, so `A[*(), int]` cannot become
   |


error[RUF077]: Unnecessary unpacking of tuple literal
  --> src/mdtest_snippet.py:15:6
   |
15 | v: A[*(), int]  # snapshot: unnecessary-literal-unpacking
   |      ^^^
   |
help: Remove unnecessary tuple
   |
14 | # `A[int]`.
   - v: A[*(), int]  # snapshot: unnecessary-literal-unpacking
15 + v: A[int,]  # snapshot: unnecessary-literal-unpacking
16 | u: A[*(), int, str]  # error: [unnecessary-literal-unpacking]
   |


error[RUF077]: Unnecessary unpacking of tuple literal
  --> src/mdtest_snippet.py:18:6
   |
18 | t: A[*((int,))]  # snapshot: unnecessary-literal-unpacking
   |      ^^^^^^^^^
   |
help: Remove unnecessary tuple
   |
17 | # Redundant parentheses and the comma a one-element slice needs, at the same time.
   - t: A[*((int,))]  # snapshot: unnecessary-literal-unpacking
18 + t: A[int,]  # snapshot: unnecessary-literal-unpacking
19 | s: A[*((int, str))]  # error: [unnecessary-literal-unpacking]
   |


error[RUF077]: Unnecessary unpacking of tuple literal
  --> src/mdtest_snippet.py:23:6
   |
23 |   r: A[*(
   |  ______^
24 | |     int,
25 | | ), str]
   | |_^
   |
help: Remove unnecessary tuple
   |
22 | # snapshot: unnecessary-literal-unpacking
   - r: A[*(
   -     int,
   - ), str]
23 + r: A[
24 +     int
25 + , str]
26 | from typing import Generic, TypeVarTuple
   |
```

The fix reaches inside a string annotation too:

```py
from typing import Generic, TypeVarTuple

Ts = TypeVarTuple("Ts")


class A(Generic[*Ts]): ...


v: "A[*(int,)]" = None  # snapshot: unnecessary-literal-unpacking
```

```snapshot
error[RUF077]: Unnecessary unpacking of tuple literal
  --> src/mdtest_snippet.py:34:7
   |
34 | v: "A[*(int,)]" = None  # snapshot: unnecessary-literal-unpacking
   |       ^^^^^^^
   |
help: Remove unnecessary tuple
   |
33 |
   - v: "A[*(int,)]" = None  # snapshot: unnecessary-literal-unpacking
34 + v: "A[int,]" = None  # snapshot: unnecessary-literal-unpacking
   |
```

## Comments the fix deletes

A comment that the fix cannot leave where it is makes the fix unsafe, since applying it loses the
comment.

### Comment between the `*` and the literal

The `*`, the gap after it, and the opening bracket all go in one deletion, so a comment written in
that gap goes too:

```py
foo = print
bar = 1

foo(
    # snapshot: unnecessary-literal-unpacking
    *  # comment
    [bar]
)
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
 --> src/mdtest_snippet.py:6:5
  |
6 | /     *  # comment
7 | |     [bar]
  | |_________^
  |
help: Remove unnecessary list
  |
5 |     # snapshot: unnecessary-literal-unpacking
  -     *  # comment
  -     [bar]
6 +     bar
7 | )
  |
note: This is an unsafe fix and may change runtime behavior
```

### Comment in the gap that removing an empty unpacking deletes

An empty unpacking goes away together with the comma that separated it from its neighbours, so a
comment written in that gap goes with it:

```py
bar = 1

# snapshot: unnecessary-literal-unpacking
values = [*[]  # comment
, bar]
values = [
    bar,  # comment
    *[],  # error: [unnecessary-literal-unpacking]
]
values = (
    bar,  # comment
    *[],  # error: [unnecessary-literal-unpacking]
)
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
 --> src/mdtest_snippet.py:4:11
  |
4 | values = [*[]  # comment
  |           ^^^
  |
help: Remove unnecessary list
  |
3 | # snapshot: unnecessary-literal-unpacking
  - values = [*[]  # comment
  - , bar]
4 + values = [bar]
5 | values = [
  |
note: This is an unsafe fix and may change runtime behavior
```

## Unfixable

### Keyword argument before the unpacking

`foo(keyword=bar, baz)` is a syntax error:

```py
foo = print
bar = 1
baz = 2

foo(keyword=bar, *[baz])  # snapshot: unnecessary-literal-unpacking
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
 --> src/mdtest_snippet.py:5:18
  |
5 | foo(keyword=bar, *[baz])  # snapshot: unnecessary-literal-unpacking
  |                  ^^^^^^
  |
help: Remove unnecessary list
```

### Keyword argument before a class base

`class C(metaclass=Meta, Base)` is a syntax error too:

```py
class Base: ...


class Meta(type): ...


class C(metaclass=Meta, *[Base]): ...  # snapshot: unnecessary-literal-unpacking
```

```snapshot
error[RUF077]: Unnecessary unpacking of list literal
 --> src/mdtest_snippet.py:7:25
  |
7 | class C(metaclass=Meta, *[Base]): ...  # snapshot: unnecessary-literal-unpacking
  |                         ^^^^^^^
  |
help: Remove unnecessary list
```

### Multi-line literal in a bare tuple

The brackets are what let the literal span several lines, since the enclosing tuple has no
parentheses of its own:

```py
bar = 1
baz = 2
qux = 3

# error: [unnecessary-literal-unpacking]
values = *[
    bar,
    baz,
], qux
```

## Not flagged

```py
def foo(*args, **kwargs): ...


bar = 1
baz = 2
rest = [3, 4]
mapping = {"a": 1}

foo(*rest)
foo(*(rest))
# Unpacking a dict yields its keys.
foo(*mapping)
foo(**mapping)
foo(*"abc")
foo(*[value for value in rest])
foo(*(value for value in rest))
[*rest]
{*rest}
(*rest,)


# A starred assignment target is not an unpacking.
first, *others = rest
[second, *more] = rest
for third, *even_more in [rest]:
    pass
```

## Type parameter defaults

The default of a `TypeVarTuple` has to be an unpacking, so its elements cannot be written out:
`class C[*Ts = int, str]` declares a second type parameter named `str` instead of giving `Ts` the
default `(int, str)`.

```toml
target-version = "py313"

[lint]
preview = true
select = ["RUF077"]
```

```py
class C[*Ts = *(int, str)]: ...


def f[*Us = *(int, str)](): ...


type X[*Vs = *(int,)] = int
```
