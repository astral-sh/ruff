# Unary addition isn't allowed but we parse it for better error recovery.
match subject:
    case +1:
        pass
    case 1 | +2 | -3:
        pass
    case [1, +2, -3]:
        pass
    case Foo(x=+1, y=-2):
        pass
    case {True: +1, False: -2}:
        pass
