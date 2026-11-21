## What it does

Checks calls to three-argument `type()` or `types.new_class()` that are assigned directly to a
single variable. If ty can infer the class name from the argument, it reports a mismatch with the
variable name.

## Why is this bad?

The name argument sets the class's `__name__`, which appears in tracebacks and representations. A
typo can make it difficult to connect that runtime name to the class definition.

## Default level

This rule is disabled by default because class factories often choose runtime names independently of
the local variables that hold their results.

## Examples

```python
from types import new_class

Point = type("Vector", (), {})  # error: [mismatched-dynamic-type-name]
Record = new_class(name="Entry")  # error: [mismatched-dynamic-type-name]
```
