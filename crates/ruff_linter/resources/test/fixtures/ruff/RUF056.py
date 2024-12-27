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

# Invalid

# Using dict.get with a falsy fallback: False
value = my_dict.get("key", False)  # [RUF056]

# Using dict.get with a falsy fallback: empty string
value = my_dict.get("key", "")  # [RUF056]

# Using dict.get with a falsy fallback: empty list
value = my_dict.get("key", [])  # [RUF056]

# Using dict.get with a falsy fallback: empty dict
value = my_dict.get("key", {})  # [RUF056]

# Using dict.get with a falsy fallback: empty set
value = my_dict.get("key", set())  # [RUF056]

# Using dict.get with a falsy fallback: zero integer
value = my_dict.get("key", 0)  # [RUF056]

# Using dict.get with a falsy fallback: zero float
value = my_dict.get("key", 0.0)  # [RUF056]

# Using dict.get with a falsy fallback: None
value = my_dict.get("key", None)  # [RUF056]

# Using dict.get with a falsy fallback via function call
value = my_dict.get("key", list())  # [RUF056]
value = my_dict.get("key", dict())  # [RUF056]
value = my_dict.get("key", set())   # [RUF056]

# Reassigning with falsy fallback
def get_value(d):
    return d.get("key", False)  # [RUF056]

# Multiple dict.get calls with mixed fallbacks
value1 = my_dict.get("key1", "default")
value2 = my_dict.get("key2", 0)  # [RUF056]
value3 = my_dict.get("key3", "another default")

# Using dict.get in a class with falsy fallback
class MyClass:
    def method(self):
        return self.data.get("key", {})  # [RUF056]

# Using dict.get in a nested function with falsy fallback
def outer():
    def inner():
        return my_dict.get("key", "")  # [RUF056]
    return inner()

# Using dict.get with variable fallback that is falsy
falsy_value = None
value = my_dict.get("key", falsy_value)  # [RUF056]

# Using dict.get with variable fallback that is truthy
truthy_value = "exists"
value = my_dict.get("key", truthy_value)

# Using dict.get with complex expressions as fallback
value = my_dict.get("key", 0 or "default")  # [RUF056]
value = my_dict.get("key", [] if condition else {})  # [RUF056] if condition

# Edge Cases

# Falsy fallback in a lambda
get_fallback = lambda d: d.get("key", False)  # [RUF056]

# Falsy fallback in a list comprehension
results = [d.get("key", "") for d in dicts]  # [RUF056]

# Falsy fallback in a generator expression
results = (d.get("key", None) for d in dicts)  # [RUF056]

# Falsy fallback in a ternary expression
value = my_dict.get("key", 0) if condition else "default"  # [RUF056]

# Falsy fallback with comments
value = my_dict.get("key", [])  # Falsy fallback used here [RUF056]

# Falsy fallback with inline comment
value = my_dict.get("key", # comment1
                     [] # comment2
                     ) # comment3
