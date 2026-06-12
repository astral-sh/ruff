## What it does

Checks for string-literal annotations where the string cannot be
parsed as a Python expression.

## Why is this bad?

Type annotations are expected to be Python expressions that
describe the expected type of a variable, parameter, attribute or
`return` statement.

Type annotations are permitted to be string-literal expressions, in
order to enable forward references to names not yet defined.
However, it must be possible to parse the contents of that string
literal as a normal Python expression.

## Example

```python
def foo() -> "intstance of C":  # error
    return 42


class C: ...
```

Use instead:

```python
def foo() -> "C":
    return C()


class C: ...
```

## References

- [Typing spec: The meaning of annotations](https://typing.python.org/en/latest/spec/annotations.html#the-meaning-of-annotations)
- [Typing spec: String annotations](https://typing.python.org/en/latest/spec/annotations.html#string-annotations)
