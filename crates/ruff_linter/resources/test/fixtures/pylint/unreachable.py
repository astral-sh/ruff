def empty_statement_reachable(): ...

def pass_statement_reachable():
    pass

def no_control_flow_reachable():
    x = 1
    x = 2
    class C:
        a = 2
    c = C()
    del c
    def foo():
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
    print("unreachable range should include above return")
