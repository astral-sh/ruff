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
