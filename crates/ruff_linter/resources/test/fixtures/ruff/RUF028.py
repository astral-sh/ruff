# fmt: on
def fmt_off_used_earlier():
    if True:
        a = 5
        with a:
            # fmt: off
            pass
    elif False:
        # fmt: off
        pass
    else:
        pass
    # fmt: off
    if True:
        # fmt: off
        pass
    # fmt: off 

# fmt: off

# fmt: on


def fmt_off_between_lists():
    test_list = [
        # fmt: off
        1,
        2,
        3,
    ]


@fmt_on_after_func
# fmt: off
@fmt_off_between_lists
def fmt_off_between_decorators():
    # fmt: skip
    pass

def fmt_on_trailing():
    # fmt: off
    val = 5 # fmt: on
    pass

def fmt_off_in_else():
    x = [1, 2, 3]
    for val in x:
        print(x)
    # fmt: off
    else:
        print("done")
    while False:
        print("while")
    # fmt: on
    # fmt: off
    else:
        print("done")
    if len(x) > 3:
        print("huh?")
    # fmt: on
    # fmt: off
    else:
        print("expected")

def dangling_fmt_off():
    pass
    # fmt: off

def dangling_fmt_off2():
    if True:
        if True:
            pass
        else:
            pass
        # fmt: off
    else:
        pass
    # fmt: off
