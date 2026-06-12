## What it does

Detects variables declared as `global` in an inner scope that have no explicit
bindings or declarations in the global scope.

## Why is this bad?

Function bodies with `global` statements can run in any order (or not at all), which makes
it hard for static analysis tools to infer the types of globals without
explicit definitions or declarations.

## Example

### Assigning without a global-scope declaration

```python
def f():
    # unresolved global
    global x  # error
    x = 42


def g():
    print(x)  # unresolved reference
```

### Use instead

#### Declare the global

```python
x: int


def f():
    global x
    x = 42


def g():
    print(x)
```

#### Initialize the global

```python
x: int | None = None


def f():
    global x
    x = 42


def g():
    print(x)
```
