# missing-type-self (ANN101)

Derived from the **flake8-annotations** linter.

## What it does
Checks that instance method `self` arguments have type annotations.

## Why is this bad?
Type annotations are a good way to document the types of function arguments. They also
help catch bugs, when used alongside a type checker, by ensuring that the types of
any provided arguments match expectation.

Note that many type checkers will infer the type of `self` automatically, so this
annotation is not strictly necessary.

## Example
```python
class Foo:
    def bar(self):
        ...
```

Use instead:
```python
class Foo:
    def bar(self: "Foo"):
        ...
```