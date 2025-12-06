class C[T: (A, B)]:
    def f(foo: T):
        try:
            pass
        except foo:
            pass
