my_dict = {}
my_var = 0


def foo():
    my_var += 1


def bar():
    global my_var
    my_var += 1


def baz():
    global my_var
    global my_dict
    my_dict[my_var] += 1


def dec(x):
    return x


@dec
def f():
    dec = 1
    return dec


class Class:
    def f(self):
        print(my_var)
        my_var = 1


class Class:
    my_var = 0

    def f(self):
        print(my_var)
        my_var = 1
