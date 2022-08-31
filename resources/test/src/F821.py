def get_name():
    return self.name


def get_name():
    return (self.name,)


def get_name():
    del self.name


def get_name(self):
    return self.name


x = list()


def randdec(maxprec, maxexp):
    return numeric_string(maxprec, maxexp)


def ternary_optarg(prec, exp_range, itr):
    for _ in range(100):
        a = randdec(prec, 2 * exp_range)
        b = randdec(prec, 2 * exp_range)
        c = randdec(prec, 2 * exp_range)
        yield a, b, c, None
        yield a, b, c, None, None


class Foo:
    CLASS_VAR = 1
    REFERENCES_CLASS_VAR = {"CLASS_VAR": CLASS_VAR}


class Class:
    def __init__(self):
        # TODO(charlie): This should be recognized as a defined variable.
        Class  # noqa: F821

try:
    x = 1 / 0
except Exception as e:
    print(e)
