# pathlib-readlink (PTH115)

Derived from the **flake8-use-pathlib** linter.

Autofix is sometimes available.

## What is does
Detects the use of `os.readlink`.
Autofix is available when the `pathlib` module is imported.

## Why is this bad?
A modern alternative to `os.readlink(x)` is the `Path(x).readlink()` function

## Examples
```python
link = os.readlink(x)
```

Use instead:
```python
link = Path(x).readlink()
```

## Options
* `target-version`
* `isort.required-imports`

## References
* [PEP 428](https://peps.python.org/pep-0428/)
* [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
* [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
* [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)