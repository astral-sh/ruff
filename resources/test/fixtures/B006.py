import collections


def f_list_literal(x=[]):
    pass


def f_dict_literal(x={}):
    pass


def f_set_literal(x={}):
    pass


def f_list_call(x=list()):
    pass


def f_dict_call(x=dict()):
    pass


def f_set_call(x=set()):
    pass


def f_ordered_dict(x=collections.OrderedDict()):
    pass


def f_counter(x=collections.Counter()):
    pass


def f_default_dict(x=collections.defaultdict()):
    pass


def f_deque(x=collections.deque()):
    pass


def f_list_comp(x=[i**2 for i in range(3)]):
    pass


def f_dict_comp(x={i: i**2 for i in range(3)}):
    pass


def f_set_comp(x={i**2 for i in range(3)}):
    pass


def f_keyword_only(*, x=[]):
    pass


def f_ok(x=None, y=()):
    pass
