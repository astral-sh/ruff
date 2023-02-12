# use-of-inplace-argument (PD002)

Derived from the **pandas-vet** linter.

Autofix is always available.

## What it does
Checks for `inplace=True` in code using the `pandas` library.

## Why is this bad?
- `inplace=True` often does not provide a performance benefit. It is
likely to copy dataframes in the background.
- It encourages mutation rather than immutable data, which is harder to
reason about and may cause bugs.
- It removes the ability to use the method chaining style for `pandas`
code.

## Example
```python
df.sort_values("col1", inplace=True)
```

Use instead:
```python
sorted_df = df.sort_values("col1")
```

## References
- [Why You Should Probably Never Use pandas inplace=True](https://towardsdatascience.com/why-you-should-probably-never-use-pandas-inplace-true-9f9f211849e4)