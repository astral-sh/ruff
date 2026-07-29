## What it does

Checks for declarations where the inferred type of an existing symbol
is not [assignable to] its post-hoc declared type.

## Why is this bad?

Such declarations break the rules of the type system and
weaken a type checker's ability to accurately reason about your code.

## Examples

```python
a = 1
a: str  # error
```

[assignable to]: https://typing.python.org/en/latest/spec/glossary.html#term-assignable
