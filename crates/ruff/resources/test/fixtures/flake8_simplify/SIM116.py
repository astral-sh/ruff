# These SHOULD change
a = "hello"

if a == "foo":
    return "bar"
elif a == "bar":
    return "baz"
elif a == "boo":
    return "ooh"
else:
    return 42

# These Should NOT change
if a == "foo":
    return "bar"
elif a == "bar":
    return baz()
elif a == "boo":
    return "ooh"
else:
    return 42
