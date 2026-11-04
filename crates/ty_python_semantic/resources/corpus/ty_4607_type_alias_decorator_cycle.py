# ty used to exceed Salsa's cycle iteration limit when checking this file.
# Minimized from py-fuzzer seed 1246: https://github.com/astral-sh/ty/issues/4607

value = lambda: value
try:
    type Alias = value
finally:
    @[Alias]
    class Example:
        pass
    value = Example
