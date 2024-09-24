# SIM910
{}.get(key, None)

# SIM910
{}.get("key", None)

# OK
{}.get(key)

# OK
{}.get("key")

# OK
{}.get(key, False)

# OK
{}.get("key", False)

# SIM910
if a := {}.get(key, None):
    pass

# SIM910
a = {}.get(key, None)

# SIM910
({}).get(key, None)

# SIM910
ages = {"Tom": 23, "Maria": 23, "Dog": 11}
age = ages.get("Cat", None)

# OK
ages = ["Tom", "Maria", "Dog"]
age = ages.get("Cat", None)

# SIM910
def foo(**kwargs):
    a = kwargs.get('a', None)

# SIM910
def foo(some_dict: dict):
    a = some_dict.get('a', None)

# OK
def foo(some_other: object):
    a = some_other.get('a', None)

# OK
def foo(some_other):
    a = some_other.get('a', None)
