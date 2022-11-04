"""Test (edge case): accesses of object named `dict`."""

# OK
dict = {"parameters": {}}
dict["parameters"]["userId"] = "userId"
dict["parameters"]["itemId"] = "itemId"
del dict["parameters"]["itemId"]

# F821 Undefined name `key`
# F821 Undefined name `value`
x: dict["key", "value"]

# OK
x: dict[str, str]
