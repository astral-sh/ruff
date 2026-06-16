## What it does

Checks for type alias definitions that (directly or mutually) refer to themselves.

## Why is it bad?

Although it is permitted to define a recursive type alias, it is not meaningful
to have a type alias whose expansion can only result in itself, and is therefore not allowed.

## Examples

```toml
[environment]
python-version = "3.12"
```

```python
type Itself = Itself  # error

type A = B  # error
type B = A  # error
```
