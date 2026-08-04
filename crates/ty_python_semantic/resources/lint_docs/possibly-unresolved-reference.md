## What it does

Checks for references to names that are possibly not defined.

## Why is this bad?

Using an undefined variable will raise a `NameError` at runtime.

## Rule status

This rule is currently disabled by default because of the number of
false positives it can produce.

## Example

```python
for i in range(int(input())):
    x = i

# NameError: name 'x' is not defined
print(x)  # error
```
