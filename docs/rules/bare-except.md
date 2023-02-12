# bare-except (E722)

Derived from the **pycodestyle** linter.

## What it does
Checks for bare `except:` in `try...except` statements.

## Why is this bad?
A bare except catches `BaseException` which includes`KeyboardInterrupt`,
`SystemExit`, `Exception` and others. It can make it hard to interrupt
the program with Ctrl+C and disguise other problems.

## Example
```python
try:
    raise(KeyboardInterrupt("You probably don't mean to break CTRL-C."))
except:
    print("But a bare except will catch BaseExceptions and break keyboard interrupts.")
```

Use instead:
```python
try:
    do_something_that_might_break()
except MoreSpecificException as e:
    handle_error(e)
```

## References
- [Pep-8 Recommendations](https://www.python.org/dev/peps/pep-0008/#programming-recommendations)
- [Python Exception Hierarchy](https://docs.python.org/3/library/exceptions.html#exception-hierarchy)
- [Google Python Style Guide on Exceptions](https://google.github.io/styleguide/pyguide.html#24-exceptions)