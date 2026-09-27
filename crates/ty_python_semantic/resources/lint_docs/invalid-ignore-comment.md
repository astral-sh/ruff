## What it does

Checks for `type: ignore` and `ty: ignore` comments that are syntactically incorrect.

## Why is this bad?

A syntactically incorrect ignore comment is probably a mistake and is useless.

## Examples

```py
# error
a = 20 / 1  # type: ignoree
```

Use instead:

```py
a = 20 / 0  # type: ignore
```
