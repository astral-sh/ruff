# missing-type-cls (ANN102)

Derived from the **flake8-annotations** linter.

## What it does
Checks that class method `cls` arguments have type annotations.

## Why is this bad?
Type annotations are a good way to document the types of function arguments. They also
help catch bugs, when used alongside a type checker, by ensuring that the types of
any provided arguments match expectation.

Note that many type checkers will infer the type of `cls` automatically, so this
annotation is not strictly necessary.

## Example
```python
class Foo:
    @classmethod
    def bar(cls):
        ...
```

Use instead:
```python
class Foo:
    @classmethod
    def bar(cls: Type["Foo"]):
        ...
```