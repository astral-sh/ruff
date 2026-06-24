def foo():
    b = 1

    def inner2():
        a

    a = 2

    def inner():
        a + b

    def inner_more():

        c = "foo"

        def inner3():
            b + a + 1 + c
