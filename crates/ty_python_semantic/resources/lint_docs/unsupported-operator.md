## What it does

Checks for binary expressions, comparisons, and unary expressions where
the operands don't support the operator.

## Why is this bad?

Attempting to use an unsupported operator will raise a `TypeError` at
runtime.

## Examples

```python
class A: ...


# TypeError: unsupported operand type(s) for +: 'A' and 'A'
A() + A()  # error
```
