# Errors.
op_bitnot = lambda x: ~x
op_not = lambda x: not x
op_pos = lambda x: +x
op_neg = lambda x: -x

op_add = lambda x, y: x + y
op_sub = lambda x, y: x - y
op_mult = lambda x, y: x * y
op_matmutl = lambda x, y: x @ y
op_truediv = lambda x, y: x / y
op_mod = lambda x, y: x % y
op_pow = lambda x, y: x ** y
op_lshift = lambda x, y: x << y
op_rshift = lambda x, y: x >> y
op_bitor = lambda x, y: x | y
op_xor = lambda x, y: x ^ y
op_bitand = lambda x, y: x & y
op_floordiv = lambda x, y: x // y

op_eq = lambda x, y: x == y
op_ne = lambda x, y: x != y
op_lt = lambda x, y: x < y
op_lte = lambda x, y: x <= y
op_gt = lambda x, y: x > y
op_gte = lambda x, y: x >= y
op_is = lambda x, y: x is y
op_isnot = lambda x, y: x is not y
op_in = lambda x, y: y in x
op_itemgetter = lambda x: x[0]
op_itemgetter = lambda x: (x[0], x[1], x[2])
op_itemgetter = lambda x: (x[1:], x[2])
op_itemgetter = lambda x: x[:]
op_itemgetter = lambda x: x[0, 1]
op_itemgetter = lambda x: x[(0, 1)]


def op_not2(x):
    return not x


def op_add2(x, y):
    return x + y


class Adder:
    def add(x, y):
        return x + y


# OK.
op_add3 = lambda x, y=1: x + y
op_neg2 = lambda x, y: y - x
op_notin = lambda x, y: y not in x
op_and = lambda x, y: y and x
op_or = lambda x, y: y or x
op_in = lambda x, y: x in y
op_itemgetter = lambda x: (1, x[1], x[2])
op_itemgetter = lambda x: (x.y, x[1], x[2])
op_itemgetter = lambda x, y: (x[0], y[0])
op_itemgetter = lambda x, y: (x[0], y[0])
op_itemgetter = lambda x: ()
op_itemgetter = lambda x: (*x[0], x[1])
op_itemgetter = lambda x: (x[0],)
op_itemgetter = lambda x: x[x]


def op_neg3(x, y):
    return y - x


def op_add4(x, y=1):
    return x + y


def op_add5(x, y):
    print("op_add5")
    return x + y


# OK
class Class:
    @staticmethod
    def add(x, y):
        return x + y
