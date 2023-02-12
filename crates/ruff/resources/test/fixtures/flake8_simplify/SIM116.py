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

if a == 1:
    return (1, 2, 3)
elif a == 2:
    return (4, 5, 6)
elif a == 3:
    return (7, 8, 9)

# These Should NOT change
if a == "foo":
    return "bar"
elif a == "bar":
    return baz()
elif a == "boo":
    return "ooh"
else:
    return 42

