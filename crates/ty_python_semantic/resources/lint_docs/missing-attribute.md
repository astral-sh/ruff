## What it does

Checks for references to attributes that ty cannot find on the object's type. For a union, this rule
applies when every member lacks the attribute. If only some members lack it, ty reports
`missing-union-attribute`.

## Why is this bad?

Accessing an unbound attribute will raise an `AttributeError` at runtime. An unresolved attribute is
not guaranteed to exist from the type alone, so this could also indicate that the object is not of
the type that the user expects.

## Examples

```python
class A: ...


# AttributeError: 'A' object has no attribute 'foo'
A().foo  # error
```

## Rule selection

This rule was renamed in ty 0.0.100. Settings such as `unresolved-attribute = "ignore"` and comments
such as `# ty: ignore[unresolved-attribute]` still apply to this rule. To configure or suppress an
access where only some union members lack the attribute, use `missing-union-attribute`.
