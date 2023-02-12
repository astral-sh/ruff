# use-of-inplace-argument (PD002)

Derived from the **pandas-vet** linter.

Autofix is always available.

## What it does
Checks for `inplace=True` usages in `pandas` function and method
calls.

## Why is this bad?
Using `inplace=True` encourages mutation rather than immutable data,
which is harder to reason about and may cause bugs. It also removes the
ability to use the method chaining style for `pandas` operations.

Further, in many cases, `inplace=True` does not provide a performance
benefit, as Pandas will often copy DataFrames in the background.

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