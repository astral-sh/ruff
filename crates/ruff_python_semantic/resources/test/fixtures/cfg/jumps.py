def only_return():
    return

def after_return():
    return 1
    print("unreachable")
    print("and this")

def after_raise():
    raise ValueError
    print("unreachable")
    print("and this")

def multiple_returns():
    return 1
    print("unreachable")
    return 2
    print("and this")
