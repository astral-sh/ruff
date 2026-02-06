# Call expressions in annotation-only variable definitions

When a variable is declared with only an annotation (no assignment) and the annotation is a call
expression, ty infers the return type of the call as the declared type of the variable.

## Class body basic

```py
def foo() -> int:
    return 1

def bar() -> str:
    return ""

class A:
    a: foo()
    b: bar()

reveal_type(A.a)  # revealed: int
reveal_type(A.b)  # revealed: str
```

## With assignment is unchanged

Call expressions in annotations with an assignment still produce an error.

```py
def foo() -> int:
    return 1

# error: [invalid-type-form] "Function calls are not allowed in type expressions"
b: foo() = 1
reveal_type(b)  # revealed: Literal[1]
```

## Class body with mixed annotations

```py
def foo() -> int:
    return 1

def bar() -> str:
    return ""

class A:
    a: foo()
    b: bar()
    c: int = 42

reveal_type(A.a)  # revealed: int
reveal_type(A.b)  # revealed: str
reveal_type(A.c)  # revealed: int
```

## Call with arguments

```py
def make(x: int, y: str) -> float:
    return 1.0

class A:
    a: make(1, "hello")

reveal_type(A.a)  # revealed: int | float
```

## No return annotation

```py
def unknown_return():
    pass

class A:
    a: unknown_return()

reveal_type(A.a)  # revealed: Unknown
```

## Function parameter annotations are unchanged

Call expressions in function parameter annotations still produce an error.

```py
def foo() -> int:
    return 1

def func(
    x: foo(),  # error: [invalid-type-form] "Function calls are not allowed in type expressions"
):
    reveal_type(x)  # revealed: Unknown
```
