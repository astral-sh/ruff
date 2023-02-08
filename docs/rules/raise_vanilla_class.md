# raise-vanilla-class (TRY002)
Derived from the **tryceratops** linter.

### What it does
Checks for bare exceptions.

## Why is this bad?
It's hard to capture generic exceptions making it hard for handling specific scenarios.

## Example
```py
def main_function():
    if not cond:
        raise Exception()
def consumer_func():
    try:
        do_step()
         prepare()
        main_function()
    except Exception:
        logger.error("I have no idea what went wrong!!")
```

## How it should be
```py
def main_function():
    if not cond:
        raise CustomException()
def consumer_func():
    try:
        do_step()
        prepare()
        main_function()
    except CustomException:
        logger.error("Main function failed")
    except Exception:
        logger.error("I have no idea what went wrong!!")
```
