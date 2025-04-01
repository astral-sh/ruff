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
