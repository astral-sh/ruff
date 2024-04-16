# Starred expression is not allowed as a mapping pattern key
match subject:
    case {*key}:
        pass
    case {*key: 1}:
        pass
    case {*key 1}:
        pass
    case {*key, None: 1}:
        pass

# Pattern cannot follow a double star pattern
# Multiple double star patterns are not allowed
match subject:
    case {**rest, None: 1}:
        pass
    case {**rest1, **rest2, None: 1}:
        pass
    case {**rest1, None: 1, **rest2}:
        pass

match subject:
    case {Foo(a as b): 1}: ...