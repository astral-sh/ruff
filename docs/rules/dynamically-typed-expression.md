# dynamically-typed-expression (ANN401)

Derived from the **flake8-annotations** linter.

### What it does
Checks that an expression is annotated with a more specific type than `Any`.

### Why is this bad?
`Any` is a type that can be anything, and it is the default type for
unannotated expressions. It is better to be explicit about the type of an
expression, and to use `Any` only when it is really needed.

### Example
```python
def foo(x: Any):
    ...
```

Use instead:
```python
def foo(x: int):
    ...
```