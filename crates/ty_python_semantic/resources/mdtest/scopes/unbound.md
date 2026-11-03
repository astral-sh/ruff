# Unbound

## Unbound class variable

Name lookups within a class scope fall back to globals, but lookups of class attributes don't.

```py
def coinflip() -> bool:
    return True

flag = coinflip()
x = 1

class C:
    y = x
    if flag:
        x = 2

# error: [possibly-missing-attribute] "Attribute `x` may be missing on class `C`"
reveal_type(C.x)  # revealed: int
reveal_type(C.y)  # revealed: int
```

## Possibly unbound in class and global scope

```py
def coinflip() -> bool:
    return True

if coinflip():
    x = "abc"

class C:
    if coinflip():
        x = 1

    # Possibly unbound variables in enclosing scopes are considered bound.
    y = x

reveal_type(C.y)  # revealed: int | str
```

## Possibly unbound in class scope with multiple declarations

```py
def coinflip() -> bool:
    return True

class C:
    if coinflip():
        x: int = 1
    elif coinflip():
        x: str = "abc"

# error: [possibly-missing-attribute]
reveal_type(C.x)  # revealed: int | str
```

## Unbound function local

An unbound function local that has definitions in the scope does not fall back to globals.

```py
x = 1

def f():
    # error: [unbound-local]
    # revealed: Unknown
    reveal_type(x)
    x = 2
    # revealed: Literal[2]
    reveal_type(x)
```

## Unbound function local named `reveal_type`

The convenience fallback for an unimported `reveal_type` only applies when name resolution does not
find the name. It does not replace an unbound local.

```py
def f():
    reveal_type(1)  # error: [unbound-local]
    reveal_type = lambda value: value
```

## Local declarations and annotations

A variable annotation makes a name local, even when the declaration does not assign a value. Python
never evaluates the annotation on a function-local variable. ty still checks it and reports that
`Later` has no definition at that point.

```toml
[environment]
python-version = "3.13"
```

```py
def f():
    value: Later  # error: [unresolved-reference]
    print(value)  # error: [unbound-local]
    class Later: ...
```

On Python 3.13, an unquoted annotation on a nested function is evaluated when the function is
defined. A quoted annotation can refer to a class defined later.

```py
def outer():
    def eager(value: Later): ...  # error: [unbound-local]
    def forward(value: "Later"): ...
    class Later: ...
```

A quoted annotation still needs a definition for the type it names.

```py
def declared_only():
    Declared: type[int]
    def forward(value: "Declared"): ...  # error: [unresolved-reference]
```

## Configuring unbound locals independently

```toml
[rules]
unresolved-reference = "ignore"
```

```py
def f():
    print(missing)
    print(local)  # snapshot: unbound-local
    print(local)  # ty: ignore[unbound-local]
    local = 1
```

```snapshot
error[unbound-local]: Name `local` used when not defined
 --> src/mdtest_snippet.py:3:11
  |
3 |     print(local)  # snapshot: unbound-local
  |           ^^^^^
```
