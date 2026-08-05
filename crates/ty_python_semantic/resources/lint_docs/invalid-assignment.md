## What it does

Checks for assignments where the type of the value
is not [assignable to] the type of the assignee.

## Why is this bad?

Such assignments break the rules of the type system and
weaken a type checker's ability to accurately reason about your code.

## Examples

```python
a: int = ""  # error
```

[assignable to]: https://typing.python.org/en/latest/spec/glossary.html#term-assignable
