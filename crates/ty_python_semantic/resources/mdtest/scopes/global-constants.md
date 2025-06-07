# Global Constants

## \_\_debug\_\_ constant

\_\_debug\_\_ constant should be globally available.

```py
reveal_type(__debug__)  # revealed: bool

def foo():
    reveal_type(__debug__)  # revealed: bool
```
