"""Test for attribute accesses on builtins."""

a = type({}).__dict__['fromkeys']
b = dict.__dict__['fromkeys']
assert a is b
