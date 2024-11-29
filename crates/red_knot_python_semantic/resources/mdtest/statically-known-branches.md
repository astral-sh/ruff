# Statically-known branches

## Always false

### If

```py
x = 1

if False:
    x = 2

reveal_type(x)  # revealed: Literal[1]
```

### Else

```py
x = 1

if True:
    pass
else:
    x = 2

reveal_type(x)  # revealed: Literal[1]
```

## Always true

### If

```py
x = 1

if True:
    x = 2

reveal_type(x)  # revealed: Literal[2]
```

### Else

```py
x = 1

if False:
    pass
else:
    x = 2

reveal_type(x)  # revealed: Literal[2]
```

## Combination

```py
x = 1

if True:
    x = 2
else:
    x = 3

reveal_type(x)  # revealed: Literal[2]
```

## Nested

```py path=nested_if_true_if_true.py
x = 1

if True:
    if True:
        x = 2
    else:
        x = 3
else:
    x = 4

reveal_type(x)  # revealed: Literal[2]
```

```py path=nested_if_true_if_false.py
x = 1

if True:
    if False:
        x = 2
    else:
        x = 3
else:
    x = 4

reveal_type(x)  # revealed: Literal[3]
```

```py path=nested_if_true_if_bool.py
def flag() -> bool: ...

x = 1

if True:
    if flag():
        x = 2
    else:
        x = 3
else:
    x = 4

reveal_type(x)  # revealed: Literal[2, 3]
```

```py path=nested_if_bool_if_true.py
def flag() -> bool: ...

x = 1

if flag():
    if True:
        x = 2
    else:
        x = 3
else:
    x = 4

reveal_type(x)  # revealed: Literal[2, 4]
```

```py path=nested_else_if_true.py
x = 1

if False:
    x = 2
else:
    if True:
        x = 3
    else:
        x = 4

reveal_type(x)  # revealed: Literal[3]
```

```py path=nested_else_if_false.py
x = 1

if False:
    x = 2
else:
    if False:
        x = 3
    else:
        x = 4

reveal_type(x)  # revealed: Literal[4]
```

```py path=nested_else_if_bool.py
def flag() -> bool: ...

x = 1

if False:
    x = 2
else:
    if flag():
        x = 3
    else:
        x = 4

reveal_type(x)  # revealed: Literal[3, 4]
```

## If-expressions

### Always true

```py
x = 1 if True else 2

reveal_type(x)  # revealed: Literal[1]
```

### Always false

```py
x = 1 if False else 2

reveal_type(x)  # revealed: Literal[2]
```

## Boolean expressions

### Always true

```py
(x := 1) == 1 or (x := 2)

reveal_type(x)  # revealed: Literal[1]
```

### Always false

```py
(x := 1) == 0 or (x := 2)

reveal_type(x)  # revealed: Literal[2]
```

## Conditional declarations

```py path=if_false.py
x: str

if False:
    x: int

def f() -> None:
    reveal_type(x)  # revealed: str
```

```py path=if_true_else.py
x: str

if True:
    pass
else:
    x: int

def f() -> None:
    reveal_type(x)  # revealed: str
```

```py path=if_true.py
x: str

if True:
    x: int

def f() -> None:
    reveal_type(x)  # revealed: int
```

```py path=if_false_else.py
x: str

if False:
    pass
else:
    x: int

def f() -> None:
    reveal_type(x)  # revealed: int
```

```py path=if_bool.py
def flag() -> bool: ...

x: str

if flag():
    x: int

def f() -> None:
    reveal_type(x)  # revealed: str | int
```

## Conditionally defined functions

```py
def f() -> int: ...
def g() -> int: ...

if True:
    def f() -> str: ...

else:
    def g() -> str: ...

reveal_type(f())  # revealed: str
reveal_type(g())  # revealed: int
```

## Conditionally defined class attributes

```py
class C:
    if True:
        x: int = 1
    else:
        x: str = "a"

reveal_type(C.x)  # revealed: int
```

## TODO

- declarations vs bindings => NoDefault: NoDefaultType
- conditional imports
- conditional class definitions
- compare with tests in if.md=>Statically known branches
- boundness
