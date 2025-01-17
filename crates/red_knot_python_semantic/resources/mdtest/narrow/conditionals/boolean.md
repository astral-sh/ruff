# Narrowing for conditionals with boolean expressions

## Narrowing in `and` conditional

```py
class A: ...
class B: ...

def _(x: A | B):
    if isinstance(x, A) and isinstance(x, B):
        reveal_type(x)  # revealed:  A & B
    else:
        reveal_type(x)  # revealed:  B & ~A | A & ~B
```

## Arms might not add narrowing constraints

```py
class A: ...
class B: ...

def _(flag: bool, x: A | B):
    if isinstance(x, A) and flag:
        reveal_type(x)  # revealed: A
    else:
        reveal_type(x)  # revealed: A | B

    if flag and isinstance(x, A):
        reveal_type(x)  # revealed: A
    else:
        reveal_type(x)  # revealed: A | B

    reveal_type(x)  # revealed: A | B
```

## Statically known arms

```py
class A: ...
class B: ...

def _(x: A | B):
    if isinstance(x, A) and True:
        reveal_type(x)  # revealed: A
    else:
        reveal_type(x)  # revealed: B & ~A

    if True and isinstance(x, A):
        reveal_type(x)  # revealed: A
    else:
        reveal_type(x)  # revealed: B & ~A

    if False and isinstance(x, A):
        # TODO: should emit an `unreachable code` diagnostic
        reveal_type(x)  # revealed: A
    else:
        reveal_type(x)  # revealed: A | B

    if False or isinstance(x, A):
        reveal_type(x)  # revealed: A
    else:
        reveal_type(x)  # revealed: B & ~A

    if True or isinstance(x, A):
        reveal_type(x)  # revealed: A | B
    else:
        # TODO: should emit an `unreachable code` diagnostic
        reveal_type(x)  # revealed: B & ~A

    reveal_type(x)  # revealed: A | B
```

## The type of multiple symbols can be narrowed down

```py
class A: ...
class B: ...

def _(x: A | B, y: A | B):
    if isinstance(x, A) and isinstance(y, B):
        reveal_type(x)  # revealed: A
        reveal_type(y)  # revealed: B
    else:
        # No narrowing: Only-one or both checks might have failed
        reveal_type(x)  # revealed: A | B
        reveal_type(y)  # revealed: A | B

    reveal_type(x)  # revealed: A | B
    reveal_type(y)  # revealed: A | B
```

## Narrowing in `or` conditional

```py
class A: ...
class B: ...
class C: ...

def _(x: A | B | C):
    if isinstance(x, A) or isinstance(x, B):
        reveal_type(x)  # revealed:  A | B
    else:
        reveal_type(x)  # revealed:  C & ~A & ~B
```

## In `or`, all arms should add constraint in order to narrow

```py
class A: ...
class B: ...
class C: ...

def _(flag: bool, x: A | B | C):
    if isinstance(x, A) or isinstance(x, B) or flag:
        reveal_type(x)  # revealed:  A | B | C
    else:
        reveal_type(x)  # revealed:  C & ~A & ~B
```

## in `or`, all arms should narrow the same set of symbols

```py
class A: ...
class B: ...
class C: ...

def _(x: A | B | C, y: A | B | C):
    if isinstance(x, A) or isinstance(y, A):
        # The predicate might be satisfied by the right side, so the type of `x` can’t be narrowed down here.
        reveal_type(x)  # revealed:  A | B | C
        # The same for `y`
        reveal_type(y)  # revealed:  A | B | C
    else:
        reveal_type(x)  # revealed:  B & ~A | C & ~A
        reveal_type(y)  # revealed:  B & ~A | C & ~A

    if (isinstance(x, A) and isinstance(y, A)) or (isinstance(x, B) and isinstance(y, B)):
        # Here, types of `x` and `y` can be narrowd since all `or` arms constraint them.
        reveal_type(x)  # revealed:  A | B
        reveal_type(y)  # revealed:  A | B
    else:
        reveal_type(x)  # revealed:  A | B | C
        reveal_type(y)  # revealed:  A | B | C
```

## mixing `and` and `not`

```py
class A: ...
class B: ...
class C: ...

def _(x: A | B | C):
    if isinstance(x, B) and not isinstance(x, C):
        reveal_type(x)  # revealed:  B & ~C
    else:
        # ~(B & ~C) -> ~B | C -> (A & ~B) | (C & ~B) | C -> (A & ~B) | C
        reveal_type(x)  # revealed: A & ~B | C
```

## mixing `or` and `not`

```py
class A: ...
class B: ...
class C: ...

def _(x: A | B | C):
    if isinstance(x, B) or not isinstance(x, C):
        reveal_type(x)  # revealed: B | A & ~C
    else:
        reveal_type(x)  # revealed: C & ~B
```

## `or` with nested `and`

```py
class A: ...
class B: ...
class C: ...

def _(x: A | B | C):
    if isinstance(x, A) or (isinstance(x, B) and not isinstance(x, C)):
        reveal_type(x)  # revealed:  A | B & ~C
    else:
        # ~(A | (B & ~C)) -> ~A & ~(B & ~C) -> ~A & (~B | C) -> (~A & C) | (~A ~ B)
        reveal_type(x)  # revealed:  C & ~A
```

## `and` with nested `or`

```py
class A: ...
class B: ...
class C: ...

def _(x: A | B | C):
    if isinstance(x, A) and (isinstance(x, B) or not isinstance(x, C)):
        # A & (B | ~C) -> (A & B) | (A & ~C)
        reveal_type(x)  # revealed:  A & B | A & ~C
    else:
        # ~((A & B) | (A & ~C)) ->
        # ~(A & B) & ~(A & ~C) ->
        # (~A | ~B) & (~A | C) ->
        # [(~A | ~B) & ~A] | [(~A | ~B) & C] ->
        # ~A | (~A & C) | (~B & C) ->
        # ~A | (C & ~B) ->
        # ~A | (C & ~B)  The positive side of ~A is  A | B | C ->
        reveal_type(x)  # revealed:  B & ~A | C & ~A | C & ~B
```

## Boolean expression internal narrowing

```py
def _(x: str | None, y: str | None):
    if x is None and y is not x:
        reveal_type(y)  # revealed: str

    # Neither of the conditions alone is sufficient for narrowing y's type:
    if x is None:
        reveal_type(y)  # revealed: str | None

    if y is not x:
        reveal_type(y)  # revealed: str | None
```
