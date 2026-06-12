## What it does

Checks for type alias definitions that (directly or mutually) refer to themselves.

## Why is it bad?

Although it is permitted to define a recursive type alias, it is not meaningful
to have a type alias whose expansion can only result in itself, and is therefore not allowed.

## Examples

```python
type Itself = Itself

type A = B
type B = A
```
