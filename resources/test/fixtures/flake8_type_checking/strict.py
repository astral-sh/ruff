def f():
    # Even in strict mode, this shouldn't rase an error, since `pkg` is used at runtime,
    # and implicitly imports `pkg.bar`.
    import pkg
    import pkg.bar

    def test(value: pkg.bar.A):
        return pkg.B()


def f():
    # Even in strict mode, this shouldn't rase an error, since `pkg.bar` is used at
    # runtime, and implicitly imports `pkg`.
    import pkg
    import pkg.bar

    def test(value: pkg.A):
        return pkg.bar.B()


def f():
    # In un-strict mode, this shouldn't rase an error, since `pkg` is used at runtime.
    import pkg
    from pkg import A

    def test(value: A):
        return pkg.B()


def f():
    # In un-strict mode, this shouldn't rase an error, since `pkg` is used at runtime.
    from pkg import A, B

    def test(value: A):
        return B()


def f():
    # Even in strict mode, this shouldn't rase an error, since `pkg.baz` is used at
    # runtime, and implicitly imports `pkg.bar`.
    import pkg.bar
    import pkg.baz

    def test(value: pkg.bar.A):
        return pkg.baz.B()


def f():
    # In un-strict mode, this _should_ rase an error, since `pkg` is used at runtime.
    import pkg
    from pkg.bar import A

    def test(value: A):
        return pkg.B()
