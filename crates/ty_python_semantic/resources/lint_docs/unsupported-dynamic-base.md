## What it does

Checks for dynamic class definitions (using `type()`) that have bases
which are unsupported by ty.

This is equivalent to `unsupported-base` but applies to classes created
via `type()` rather than `class` statements.

## Why is this bad?

If a dynamically created class has a base that is an unsupported type
such as `type[T]`, ty will not be able to resolve the
[method resolution order] (MRO) for the class. This may lead to an inferior
understanding of your codebase and unpredictable type-checking behavior.

## Default level

This rule is disabled by default because it will not cause a runtime error,
and may be noisy on codebases that use `type()` in highly dynamic ways.

## Examples

```python
class Base: ...


def factory(base: type[Base]) -> type:
    # `base` has type `type[Base]`, not `type[Base]` itself
    return type("Dynamic", (base,), {})  # error: [unsupported-dynamic-base]
```

[method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
