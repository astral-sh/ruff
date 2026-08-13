# Correct

my_dict = {"key": "value", "other_key": "other_value"}

# Using dict.get without a fallback
value = my_dict.get("key")

# Using dict.get with a truthy fallback
value = my_dict.get("key", "default")
value = my_dict.get("key", 42)
value = my_dict.get("key", [1, 2, 3])
value = my_dict.get("key", {"nested": "dict"})
value = my_dict.get("key", set([1, 2]))
value = my_dict.get("key", True)
value = my_dict.get("key", "Non-empty string")
value = my_dict.get("key", 3.14)
value = my_dict.get("key", (1, 2))  # Tuples are truthy

# Chained get calls with truthy fallbacks
value1 = my_dict.get("key1", {'k': 'v'}).get("subkey")
value2 = my_dict.get("key2", [1, 2, 3]).append(4)

# Valid

# Using dict.get with a falsy fallback: False
value = my_dict.get("key", False)

# Using dict.get with a falsy fallback: empty string
value = my_dict.get("key", "")

# Using dict.get with a falsy fallback: empty list
value = my_dict.get("key", [])

# Using dict.get with a falsy fallback: empty dict
value = my_dict.get("key", {})

# Using dict.get with a falsy fallback: empty set
value = my_dict.get("key", set())

# Using dict.get with a falsy fallback: zero integer
value = my_dict.get("key", 0)

# Using dict.get with a falsy fallback: zero float
value = my_dict.get("key", 0.0)

# Using dict.get with a falsy fallback: None
value = my_dict.get("key", None)

# Using dict.get with a falsy fallback via function call
value = my_dict.get("key", list())
value = my_dict.get("key", dict())
value = my_dict.get("key", set())

# Reassigning with falsy fallback
def get_value(d):
    return d.get("key", False)

# Multiple dict.get calls with mixed fallbacks
value1 = my_dict.get("key1", "default")
value2 = my_dict.get("key2", 0)
value3 = my_dict.get("key3", "another default")

# Using dict.get in a class with falsy fallback
class MyClass:
    def method(self):
        return self.data.get("key", {})

# Using dict.get in a nested function with falsy fallback
def outer():
    def inner():
        return my_dict.get("key", "")
    return inner()

# Using dict.get with variable fallback that is falsy
falsy_value = None
value = my_dict.get("key", falsy_value)

# Using dict.get with variable fallback that is truthy
truthy_value = "exists"
value = my_dict.get("key", truthy_value)

# Using dict.get with complex expressions as fallback
value = my_dict.get("key", 0 or "default")
value = my_dict.get("key", [] if condition else {})

# testing dict.get call using kwargs
value = my_dict.get(key="key", default=False)
value = my_dict.get(default=[], key="key")


# Edge Cases

dicts = [my_dict, my_dict, my_dict]

# Falsy fallback in a lambda
get_fallback = lambda d: d.get("key", False)

# Falsy fallback in a list comprehension
results = [d.get("key", "") for d in dicts]

# Falsy fallback in a generator expression
results = (d.get("key", None) for d in dicts)

# Falsy fallback in a ternary expression
value = my_dict.get("key", 0) if True else "default"


# Falsy fallback with inline comment
value = my_dict.get("key", # comment1
                     [] # comment2
                     ) # comment3

# Invalid
# Invalid falsy fallbacks are when the call to dict.get is used in a boolean context

# dict.get in ternary expression
value = "not found" if my_dict.get("key", False) else "default"  # [RUF056]

# dict.get in an if statement
if my_dict.get("key", False):  # [RUF056]
    pass

# dict.get in compound if statement
if True and my_dict.get("key", False):  # [RUF056]
    pass

if my_dict.get("key", False) or True:  # [RUF056]
    pass

# dict.get in an assert statement
assert my_dict.get("key", False)  # [RUF056]

# dict.get in a while statement
while my_dict.get("key", False):  # [RUF056]
    pass

# dict.get in unary not expression
value = not my_dict.get("key", False)  # [RUF056]

# testing all falsy fallbacks
value = not my_dict.get("key", False)  # [RUF056]
value = not my_dict.get("key", [])  # [RUF056]
value = not my_dict.get("key", list())  # [RUF056]
value = not my_dict.get("key", {})  # [RUF056]
value = not my_dict.get("key", dict())  # [RUF056]
value = not my_dict.get("key", set())  # [RUF056]
value = not my_dict.get("key", None)  # [RUF056]
value = not my_dict.get("key", 0)  # [RUF056]
value = not my_dict.get("key", 0.0)  # [RUF056]
value = not my_dict.get("key", "")  # [RUF056]

# testing invalid dict.get call with inline comment
value = not my_dict.get("key", # comment1
                     [] # comment2
                     ) # [RUF056]

# regression tests for https://github.com/astral-sh/ruff/issues/18628
# we should avoid fixes when there are "unknown" arguments present, including
# extra positional arguments, either of the positional-only arguments passed as
# a keyword, or completely unknown keywords.

# extra positional
not my_dict.get("key", False, "?!")

# `default` is positional-only, so these are invalid
not my_dict.get("key", default=False)
not my_dict.get(key="key", default=False)
not my_dict.get(default=[], key="key")
not my_dict.get(default=False)
not my_dict.get(key="key", other="something", default=False)
not my_dict.get(default=False, other="something", key="test")

# comments don't really matter here because of the kwargs but include them for
# completeness
not my_dict.get(
    key="key",  # comment1
    default=False,  # comment2
)  # comment 3
not my_dict.get(
    default=[],  # comment1
    key="key",  # comment2
)  # comment 3

# the fix is arguably okay here because the same `takes no keyword arguments`
# TypeError is raised at runtime before and after the fix, but we still bail
# out for having an unrecognized number of arguments
not my_dict.get("key", False, foo=...)

# https://github.com/astral-sh/ruff/issues/18798
d = {}
not d.get("key", (False))
