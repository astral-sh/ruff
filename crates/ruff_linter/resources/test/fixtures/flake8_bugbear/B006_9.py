def f1(x=([],)):
    print(x)


def f2(x=(x for x in "x")):
    print(x)


def f3(x=((x for x in "x"),)):
    print(x)


def f4(x=(z := [1, ])):
    print(x)


def f5(x=([1, ])):
    print(x)


def w1(x=(1,)):
    print(x)


def w2(x=(z := 3)):
    print(x)
