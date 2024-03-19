# Invalid keyword pattern in class argument
match subject:
    case Foo(x as y = 1):
        pass
    case Foo(x | y = 1):
        pass
    case Foo([x, y] = 1):
        pass
    case Foo({False: 0} = 1):
        pass
    case Foo(1=1):
        pass
    case Foo(Bar()=1):
        pass
    # Positional pattern cannot follow keyword pattern
    # case Foo(x, y=1, z):
    #     pass
