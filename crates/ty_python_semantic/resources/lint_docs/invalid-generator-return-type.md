## What it does

Checks whether a generator function's return annotation allows the generator object it produces.
This applies to both synchronous and asynchronous generators.

## Why is this bad?

A caller that relies on an incompatible annotation can fail at runtime. For example, calling a
generator function annotated with `-> int` produces a generator object even if every value it yields
is an integer.

## Examples

```python
from collections.abc import Generator


# error: [invalid-generator-return-type]
def numbers() -> int:
    yield 1


# error: [invalid-generator-return-type]
async def async_numbers() -> Generator[int]:
    yield 1
```

Both functions yield integers. Annotate `numbers` with `collections.abc.Generator[int]` and
`async_numbers` with `collections.abc.AsyncGenerator[int]`.

An incompatible explicit `return` inside a synchronous generator is reported by
`invalid-explicit-return`.

## Rule history

- v0.0.101: Split from `invalid-return-type`.
