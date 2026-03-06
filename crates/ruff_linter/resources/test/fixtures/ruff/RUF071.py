# Errors

d = {}

# Basic single-item dict literal update
d.update({"key": "value"})  # RUF071

# Different key/value types
d.update({1: 2})  # RUF071
d.update({True: False})  # RUF071
d.update({"key": 42})  # RUF071
d.update({"key": [1, 2, 3]})  # RUF071
d.update({"key": None})  # RUF071

# Variable as key
x = "key"
d.update({x: "value"})  # RUF071

# Variable as value
y = "value"
d.update({"key": y})  # RUF071


# OK

# Multiple items - not covered by this rule
d.update({"key1": "value1", "key2": "value2"})

# Empty dict
d.update({})

# Keyword argument form
d.update(key="value")

# Other dict argument forms
other = {"key": "value"}
d.update(other)

# Dict unpacking inside the literal
d.update({**other})

# Not a dict.update call
d.clear()
d.setdefault("key", "value")

# Not a known dict
unknown_obj.update({"key": "value"})

# Used as expression (not standalone statement)
result = d.update({"key": "value"})

# Multiple positional arguments (invalid, but don't flag)
# d.update({"key": "value"}, extra)
