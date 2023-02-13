# builtin-variable-shadowing (A001)

Derived from the **flake8-builtins** linter.

## What it does
Checks for any variable (and function) assignments that have the same
name as a builtin.

Keep in mind that this also takes into account the [`builtins`] and
[`flake8-builtins.builtins-ignorelist`] configuration options.

## Why is this bad?
Using a builtin name as the name of a variable increases
the difficulty of reading and maintaining the code, can cause
non-obvious code errors, and can mess up code highlighters.

Instead, the variable should be renamed to something else
that is not considered a builtin. If you are sure that you want
to name the variable this way, you can also edit the [`flake8-builtins.builtins-ignorelist`]
configuration option.

## Options

* [`builtins`]
* [`flake8-builtins.builtins-ignorelist`]

## Example
```python
def find_max(list_of_lists):
    max = 0
    for flat_list in list_of_lists:
        for value in flat_list:
            # This is confusing, and causes an error!
            max = max(max, value)  # TypeError: 'int' object is not callable
    return max
```

Use instead:
```python
def find_max(list_of_lists):
    result = 0
    for flat_list in list_of_lists:
        for value in flat_list:
            result = max(result, value)
    return result
```

* [StackOverflow - Why is it a bad idea to name a variable `id` in Python?](https://stackoverflow.com/questions/77552/id-is-a-bad-variable-name-in-python)

[`builtins`]: ../../settings#builtins[`flake8-builtins.builtins-ignorelist`]: ../../settings#builtins-ignorelist