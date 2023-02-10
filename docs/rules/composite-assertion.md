# composite-assertion (PT018)

Derived from the **flake8-pytest-style** linter.

Autofix is sometimes available.

## What it does
This violation is reported when the plugin encounter an assertion on multiple conditions.

## Why is this bad?
Composite assertion statements are harder to understand and to debug when failures occur.

## Example
```python
def test_foo():
    assert something and something_else

def test_bar():
    assert not (something or something_else)
```

Use instead:
```python
def test_foo():
    assert something
    assert something_else

def test_bar():
    assert not something
    assert not something_else
```