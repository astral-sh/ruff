# Narrowing for conditionals with boolean expressions

## Narrowing in `and` conditional

```py
class A: ...
class B: ...

def instance() -> A | B:
    return A()

x = instance()

if isinstance(x, A) and isinstance(x, B):
    reveal_type(x)  # revealed:  A & B
else:
    reveal_type(x)  # revealed:  B & ~A | A & ~B
```

## Arms might not add narrowing constraints

```py
class A: ...
class B: ...

def instance() -> A | B:
    return A()

x = instance()

if isinstance(x, A) and True:
    reveal_type(x)  # revealed: A
else:
    reveal_type(x)  # revealed: A | B

if True and isinstance(x, A):
    reveal_type(x)  # revealed: A
else:
    reveal_type(x)  # revealed: A | B

reveal_type(x)  # revealed: A | B
```

## The type of multiple symbols can be narrowed down

```py
class A: ...
class B: ...

def instance() -> A | B:
    return A()

x = instance()
y = instance()

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

def instance() -> A | B | C:
    return A()

x = instance()

if isinstance(x, A) or isinstance(x, B):
    reveal_type(x)  # revealed:  A | B
else:
    # TODO: Should be simplified to C
    reveal_type(x)  # revealed:  C & ~A & ~B
```

## In `or`, all arms should add constraint in order to narrow

```py
class A: ...
class B: ...
class C: ...

def instance() -> A | B | C:
    return A()

def bool_instance() -> bool:
    return True

x = instance()

if isinstance(x, A) or isinstance(x, B) or bool_instance():
    reveal_type(x)  # revealed:  A | B | C
else:
    reveal_type(x)  # revealed:  C & ~A & ~B
```

## in `or`, all arms should narrow the same symbol

```py
class A: ...
class B: ...
class C: ...

def instance() -> A | B | C:
    return A()

x = instance()
y = instance()

if isinstance(x, A) or isinstance(y, B):
    reveal_type(x)  # revealed:  A | B | C
    reveal_type(y)  # revealed:  A | B | C
else:
    reveal_type(x)  # revealed:  B & ~A | C & ~A
    reveal_type(y)  # revealed:  A & ~B | C & ~B
```

## mixing `and` and `not`

```py
class A: ...
class B: ...
class C: ...

def instance() -> A | B | C:
    return A()

x = instance()

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

def instance() -> A | B | C:
    return A()

x = instance()

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

def instance() -> A | B | C:
    return A()

x = instance()

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

def instance() -> A | B | C:
    return A()

x = instance()

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
def optional_string() -> str | None:
    return None

x = optional_string()
y = optional_string()

if x is None and y is not x:
    reveal_type(y)  # revealed: str

# Neither of the conditions alone is sufficient for narrowing y's type:
if x is None:
    reveal_type(y)  # revealed: str | None

if y is not x:
    reveal_type(y)  # revealed: str | None
```
