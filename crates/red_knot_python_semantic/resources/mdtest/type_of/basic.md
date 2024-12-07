# type special form

## Class literal

```py
class A: ...

def f() -> type[A]:
    return A

reveal_type(f())  # revealed: type[A]
```

## Nested class literal

```py
class A:
    class B: ...

def f() -> type[A.B]:
    return A.B

reveal_type(f())  # revealed: type[B]
```

## Deeply nested class literal

```py
class A:
    class B:
        class C: ...

def f() -> type[A.B.C]:
    return A.B.C

reveal_type(f())  # revealed: type[C]
```

## Class literal from another module

```py
from a import A

def f() -> type[A]:
    return A

reveal_type(f())  # revealed: type[A]
```

```py path=a.py
class A: ...
```

## Qualified class literal from another module

```py
import a

def f() -> type[a.B]:
    return a.B

reveal_type(f())  # revealed: type[B]
```

```py path=a.py
class B: ...
```

## Deeply qualified class literal from another module

```py path=a/test.py
import a.b

# TODO: no diagnostic
# error: [unresolved-attribute]
def f() -> type[a.b.C]:
    # TODO: no diagnostic
    # error: [unresolved-attribute]
    return a.b.C

reveal_type(f())  # revealed: @Todo(unsupported type[X] special form)
```

```py path=a/__init__.py
```

```py path=a/b.py
class C: ...
```
