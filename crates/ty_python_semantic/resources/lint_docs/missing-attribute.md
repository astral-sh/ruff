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

## Rule history

- v0.0.100: ty used to have a rule called `unresolved-attribute` that covered both
    `missing-attribute` and `missing-union-attribute`.

    The old `unresolved-attribute` name is now an alias for `missing-attribute` only. If you
    previously had any configuration for `unresolved-attribute` specifically, you should go through
    `missing-union-attribute` to see if you want to configure it in the same way.
