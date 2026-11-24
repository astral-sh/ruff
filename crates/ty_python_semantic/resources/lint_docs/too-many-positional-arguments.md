## What it does

Checks for calls that pass more positional arguments than the callable can accept.

## Why is this bad?

Passing too many positional arguments will raise `TypeError` at runtime.

## Example

```python
def f(): ...


f("foo")  # error
```
