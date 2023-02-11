def f():
    with Nested(m) as (foo):
        pass

def f():
    # Issue #23192: Test that a lambda returning a generator behaves
    # like the equivalent function
    f = lambda: (yield 1)
    def g(): return (yield 1)

    # test 'yield from'
    f2 = lambda: (yield from g())
    def g2(): return (yield from g())

    f3 = lambda: (yield from f())

def f():
    # Remove `toplevel`.
    toplevel = tt = lexer.get_token()
    if not tt:
        break
