## What it does

Detects functions with stub-like bodies whose return type excludes `None`. ty accepts the old name
`empty-body` and warns about the rename in configuration and inline ignores.

The errors reported by this rule have the same motivation as the `invalid-implicit-return` rule. The
diagnostic exists as a separate error code to allow users to disable this rule while prototyping
code. While we strongly recommend enabling this rule if possible, users migrating from other type
checkers may also find it useful to temporarily disable this rule on some or all of their codebase
if they find it results in a large number of diagnostics.

## Why is this bad?

A function with an empty body (containing only `...`, `pass`, or a docstring) will implicitly return
`None` at runtime. Returning `None` when the return type is non-`None` is unsound, and will lead to
ty inferring incorrect types elsewhere.

Functions with empty bodies are permitted in certain contexts where they serve as declarations
rather than implementations:

- Functions in stub files (`.pyi`)
- Methods in Protocol classes
- Abstract methods decorated with `@abstractmethod`
- Overload declarations decorated with `@overload`
- Functions in `if TYPE_CHECKING` blocks

## Examples

```python
def foo() -> int: ...  # error: [invalid-implicit-return-in-stub-function]


def bar() -> str:  # error: [invalid-implicit-return-in-stub-function]
    """A function that does nothing."""
    pass
```

## Rule history

- v0.0.14: Split from `invalid-return-type` as `empty-body`.
- v0.0.101: Renamed from `empty-body` to `invalid-implicit-return-in-stub-function`.
