# `unnecessary-literal-unpacking` (`PIE811`)

```toml
[lint]
preview = true
select = ["PIE811"]
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
error[PIE811]: Unnecessary unpacking of list literal
 --> src/mdtest_snippet.py:8:5
  |
8 | foo(*[bar])  # snapshot: unnecessary-literal-unpacking
  |     ^^^^^^
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
error[PIE811]: Unnecessary unpacking of list literal
 --> src/mdtest_snippet.py:5:2
  |
5 | [*[bar, baz], *rest]  # snapshot: unnecessary-literal-unpacking
  |  ^^^^^^^^^^^
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
error[PIE811]: Unnecessary unpacking of list literal
 --> src/mdtest_snippet.py:4:10
  |
4 | values = *[bar],  # snapshot: unnecessary-literal-unpacking
  |          ^^^^^^
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
[*([bar, baz]), bar]  # error: [unnecessary-literal-unpacking]
values = *([bar]),  # error: [unnecessary-literal-unpacking]
```

```snapshot
error[PIE811]: Unnecessary unpacking of list literal
 --> src/mdtest_snippet.py:5:5
  |
5 | foo(*([bar, baz]))  # snapshot: unnecessary-literal-unpacking
  |     ^^^^^^^^^^^^^
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
error[PIE811]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:16:5
   |
16 |   foo(*([bar]  # comment
   |  _____^
17 | | ))
   | |_^
help: Remove unnecessary list
   |
15 | # snapshot: unnecessary-literal-unpacking
   - foo(*([bar]  # comment
   - ))
16 + foo(bar  # comment
17 + )
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
```

```snapshot
error[PIE811]: Unnecessary unpacking of list literal
 --> src/mdtest_snippet.py:7:10
  |
7 | class C1(*[Base]): ...  # snapshot: unnecessary-literal-unpacking
  |          ^^^^^^^
help: Remove unnecessary list
  |
6 |
  - class C1(*[Base]): ...  # snapshot: unnecessary-literal-unpacking
7 + class C1(Base): ...  # snapshot: unnecessary-literal-unpacking
8 |
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
error[PIE811]: Unnecessary unpacking of set literal
 --> src/mdtest_snippet.py:6:5
  |
6 | foo(*{bar})  # snapshot: unnecessary-literal-unpacking
  |     ^^^^^^
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
error[PIE811]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:7:5
   |
 7 | /     *[
 8 | |         # leading comment
 9 | |         bar,  # trailing comment
10 | |         baz,
11 | |     ]
   | |_____^
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

## Subscripts

```toml
target-version = "py311"

[lint]
preview = true
select = ["PIE811"]
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
w: A[*()]  # error: [unnecessary-literal-unpacking]
v: A[*(), int]  # error: [unnecessary-literal-unpacking]
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
error[PIE811]: Unnecessary unpacking of tuple literal
 --> src/mdtest_snippet.py:9:6
  |
9 | x: A[*(int,)]  # snapshot: unnecessary-literal-unpacking
  |      ^^^^^^^
help: Remove unnecessary tuple
   |
8  |
   - x: A[*(int,)]  # snapshot: unnecessary-literal-unpacking
9  + x: A[int,]  # snapshot: unnecessary-literal-unpacking
10 | y: A[*(int, str)]  # snapshot: unnecessary-literal-unpacking
   |


error[PIE811]: Unnecessary unpacking of tuple literal
  --> src/mdtest_snippet.py:10:6
   |
10 | y: A[*(int, str)]  # snapshot: unnecessary-literal-unpacking
   |      ^^^^^^^^^^^
help: Remove unnecessary tuple
   |
9  | x: A[*(int,)]  # snapshot: unnecessary-literal-unpacking
   - y: A[*(int, str)]  # snapshot: unnecessary-literal-unpacking
10 + y: A[int, str]  # snapshot: unnecessary-literal-unpacking
11 | z: A[*(int,), str]  # error: [unnecessary-literal-unpacking]
   |


error[PIE811]: Unnecessary unpacking of tuple literal
  --> src/mdtest_snippet.py:15:6
   |
15 | t: A[*((int,))]  # snapshot: unnecessary-literal-unpacking
   |      ^^^^^^^^^
help: Remove unnecessary tuple
   |
14 | # Redundant parentheses and the comma a one-element slice needs, at the same time.
   - t: A[*((int,))]  # snapshot: unnecessary-literal-unpacking
15 + t: A[int,]  # snapshot: unnecessary-literal-unpacking
16 | s: A[*((int, str))]  # error: [unnecessary-literal-unpacking]
   |


error[PIE811]: Unnecessary unpacking of tuple literal
  --> src/mdtest_snippet.py:20:6
   |
20 |   r: A[*(
   |  ______^
21 | |     int,
22 | | ), str]
   | |_^
help: Remove unnecessary tuple
   |
19 | # snapshot: unnecessary-literal-unpacking
   - r: A[*(
   -     int,
   - ), str]
20 + r: A[
21 +     int
22 + , str]
23 | from typing import Generic, TypeVarTuple
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
error[PIE811]: Unnecessary unpacking of tuple literal
  --> src/mdtest_snippet.py:31:7
   |
31 | v: "A[*(int,)]" = None  # snapshot: unnecessary-literal-unpacking
   |       ^^^^^^^
help: Remove unnecessary tuple
   |
30 |
   - v: "A[*(int,)]" = None  # snapshot: unnecessary-literal-unpacking
31 + v: "A[int,]" = None  # snapshot: unnecessary-literal-unpacking
   |
```

## Empty literals

An empty literal has no elements to write out, so the unpacking goes away entirely: the removal takes
a neighbouring comma along with it, and can leave the surrounding collection needing to be rewritten.
The unpacking is still reported, but no fix is offered for it.

```py
def foo(*args): ...


bar = 1

foo(*[])  # snapshot: unnecessary-literal-unpacking
foo(*(), bar)  # error: [unnecessary-literal-unpacking]
foo(*([]))  # error: [unnecessary-literal-unpacking]
[bar, *[]]  # error: [unnecessary-literal-unpacking]
{bar, *[]}  # error: [unnecessary-literal-unpacking]
(*[], bar)  # error: [unnecessary-literal-unpacking]
(*[],)  # error: [unnecessary-literal-unpacking]
values = *[],  # error: [unnecessary-literal-unpacking]
# The outer set is not reported: a set built from a spread still deduplicates.
{*[]}  # error: [unnecessary-literal-unpacking]


class C(*[]): ...  # error: [unnecessary-literal-unpacking]
```

```snapshot
error[PIE811]: Unnecessary unpacking of list literal
 --> src/mdtest_snippet.py:6:5
  |
6 | foo(*[])  # snapshot: unnecessary-literal-unpacking
  |     ^^^
help: Remove unnecessary list
```

An unpacking that does have elements is still fixed when an empty one sits beside it in the same
display. The empty one stays a single element of the display, so only its neighbour is expanded:

```py
bar = 1

# error: [unnecessary-literal-unpacking]
# snapshot: unnecessary-literal-unpacking
(*[], *[bar])
```

```snapshot
error[PIE811]: Unnecessary unpacking of list literal
  --> src/mdtest_snippet.py:23:7
   |
23 | (*[], *[bar])
   |       ^^^^^^
help: Remove unnecessary list
   |
22 | # snapshot: unnecessary-literal-unpacking
   - (*[], *[bar])
23 + (*[], bar)
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
error[PIE811]: Unnecessary unpacking of list literal
 --> src/mdtest_snippet.py:6:5
  |
6 | /     *  # comment
7 | |     [bar]
  | |_________^
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
error[PIE811]: Unnecessary unpacking of list literal
 --> src/mdtest_snippet.py:5:18
  |
5 | foo(keyword=bar, *[baz])  # snapshot: unnecessary-literal-unpacking
  |                  ^^^^^^
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
error[PIE811]: Unnecessary unpacking of list literal
 --> src/mdtest_snippet.py:7:25
  |
7 | class C(metaclass=Meta, *[Base]): ...  # snapshot: unnecessary-literal-unpacking
  |                         ^^^^^^^
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
select = ["PIE811"]
```

```py
class C[*Ts = *(int, str)]: ...


def f[*Us = *(int, str)](): ...


type X[*Vs = *(int,)] = int
```
