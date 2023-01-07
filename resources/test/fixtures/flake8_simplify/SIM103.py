# Bad
if a:
    return True
else:
    return False

# Good
if a:
    foo()
    return True
else:
    return False

if a:
    return "foo"
else:
    return False
