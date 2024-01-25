"""
Test for else-if-used
"""


def ok0():
    """Should not trigger on elif"""
    if 1:
        pass
    elif 2:
        pass


def ok1():
    """If the orelse has more than 1 item in it, shouldn't trigger"""
    if 1:
        pass
    else:
        print()
        if 1:
            pass


def ok2():
    """If the orelse has more than 1 item in it, shouldn't trigger"""
    if 1:
        pass
    else:
        if 1:
            pass
        print()


def not_ok0():
    if 1:
        pass
    else:
        if 2:
            pass


def not_ok1():
    if 1:
        pass
    else:
        if 2:
            pass
        else:
            pass


def not_ok1_with_comments():
    if 1:
        pass
    else:
        # inner comment
        if 2:
            pass
        else:
            pass  # final pass comment


# Regression test for https://github.com/apache/airflow/blob/f1e1cdcc3b2826e68ba133f350300b5065bbca33/airflow/models/dag.py#L1737
def not_ok2():
    if True:
        print(1)
    elif True:
        print(2)
    else:
        if True:
            print(3)
        else:
            print(4)


def not_ok3():
    if 1:
        pass
    else:
        if 2: pass
        else: pass


def not_ok4():
    if 1:
        pass
    else:
        if 2: pass
        else:
            pass


def not_ok5():
    if 1:
        pass
    else:
        if 2:
            pass
        else: pass
