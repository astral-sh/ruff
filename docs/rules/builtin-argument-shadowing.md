# builtin-argument-shadowing (A002)

Derived from the **flake8-builtins** linter.

## What it does
Checks for any function arguments that have the same name as a builtin.

Keep in mind that this also takes into account the [`builtins`] and
[`flake8-builtins.builtins-ignorelist`] configuration options.

## Why is this bad?
Using a builtin name as the name of an argument name increases
the difficulty of reading and maintaining the code, can cause
non-obvious code errors, and can mess up code highlighters.

Instead, the function argument should be renamed to something else
that is not considered a builtin. If you are sure that you want
to name the argument this way, you can also edit the [`flake8-builtins.builtins-ignorelist`]
configuration option.

## Options

* [`builtins`]
* [`flake8-builtins.builtins-ignorelist`]

## Example
```python
def remove_duplicates(list, list2):
    result = set()
    for value in list:
        result.add(value)
    for value in list2:
        result.add(value)
    return list(result)  # TypeError: 'list' object is not callable
```

Use instead:
```python
def remove_duplicates(list1, list2):
    result = set()
    for value in list1:
        result.add(value)
    for value in list2:
        result.add(value)
    return list(result)  
```

## References
- [StackOverflow - Is it bad practice to use a built-in function name as an attribute or method identifier?](https://stackoverflow.com/questions/9109333/is-it-bad-practice-to-use-a-built-in-function-name-as-an-attribute-or-method-ide)
- [StackOverflow - Why is it a bad idea to name a variable `id` in Python?](https://stackoverflow.com/questions/77552/id-is-a-bad-variable-name-in-python)

[`builtins`]: ../../settings#builtins[`flake8-builtins.builtins-ignorelist`]: ../../settings#builtins-ignorelist