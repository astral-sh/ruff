# use-of-inplace-argument (PD002)

Derived from the **pandas-vet** linter.

Autofix is always available.

## What it does
Checks for `inplace=True` inside `pandas` code.

## Why is this bad?
- Many people expect `inplace=True` to be a performance benefit that prevents dataframe copies, but that's often not true.
- It encourages mutation rather than immutable data, which is harder to reason about and may cause bugs.
- It removes the ability to use the chaining style for `pandas` code.

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