# builtin-attribute-shadowing (A003)

Derived from the **flake8-builtins** linter.

## What it does
Checks for any class attributes that have the same name as a builtin.

Keep in mind that this also takes into account the [`builtins`] and
[`flake8-builtins.builtins-ignorelist`] configuration options.

## Why is this bad?
Using a builtin name as the name of a class attribute increases
the difficulty of reading and maintaining the code, can cause
non-obvious code errors, and can mess up code highlighters.

Instead, the attribute should be renamed to something else
that is not considered a builtin or converted to the related dunder
(aka magic) method.If you are sure that you want to name the attribute
this way, you can also edit the [`flake8-builtins.builtins-ignorelist`] configuration option.

## Options

* [`builtins`]
* [`flake8-builtins.builtins-ignorelist`]

## Example
```python
class Shadow:
    def int():
        return 0
```

Use instead:
```python
class Shadow:
    def to_int():
        return 0
    # OR (keep in mind you will have to use `int(shadow)` instead of `shadow.int()`)
    def __int__():
        return 0
```

## References
- [StackOverflow - Is it bad practice to use a built-in function name as an attribute or method identifier?](https://stackoverflow.com/questions/9109333/is-it-bad-practice-to-use-a-built-in-function-name-as-an-attribute-or-method-ide)
- [StackOverflow - Why is it a bad idea to name a variable `id` in Python?](https://stackoverflow.com/questions/77552/id-is-a-bad-variable-name-in-python)

[`builtins`]: ../../settings#builtins[`flake8-builtins.builtins-ignorelist`]: ../../settings#builtins-ignorelist