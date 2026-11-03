## What it does

Checks for calls to non-callable objects.

Before ty 0.0.97, this rule also covered direct instantiation of abstract classes and protocols. To
keep a custom severity for those calls, set `instantiate-abstract-class` to that severity too. For
an inline ignore that suppresses such a call, use `# ty: ignore[instantiate-abstract-class]`.

## Why is this bad?

Calling a non-callable object will raise a `TypeError` at runtime.

## Example

```python
# TypeError: 'int' object is not callable
4()  # error
```
