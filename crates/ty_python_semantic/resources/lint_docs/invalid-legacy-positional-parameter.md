## What it does

Checks for parameters that appear to be attempting to use the legacy convention
to specify that a parameter is positional-only, but do so incorrectly.

The "legacy convention" for specifying positional-only parameters was
specified in [PEP 484][pep-484]. It states that parameters with names starting with
`__` should be considered positional-only by type checkers. [PEP 570][pep-570], introduced
in Python 3.8, added dedicated syntax for specifying positional-only parameters,
rendering the legacy convention obsolete. However, some codebases may still
use the legacy convention for compatibility with older Python versions.

## Why is this bad?

In most cases, a type checker will not consider a parameter to be positional-only
if it comes after a positional-or-keyword parameter, even if its name starts with
`__`. This may be unexpected to the author of the code.

## Example

```python
# `__y` is not considered positional-only
def f(x, __y):  # error
    pass
```

Use instead:

```python
def f(__x, __y):  # If you need compatibility with Python <=3.7
    pass
```

or:

```python
def f(x, y, /):  # Python 3.8+ syntax
    pass
```

## References

- [Typing spec: positional-only parameters (legacy syntax)](https://typing.python.org/en/latest/spec/historical.html#pos-only-double-underscore)
- [Python glossary: parameters](https://docs.python.org/3/glossary.html#term-parameter)

[pep-484]: https://peps.python.org/pep-0484/#positional-only-arguments
[pep-570]: https://peps.python.org/pep-0570/
