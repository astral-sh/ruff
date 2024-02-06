
def fmt_off_in_elif():
    if True:
        pass
    elif False:
        pass
# fmt: on

def fmt_off_between_lists():
    test_list = [
        #fmt: off
        1,
        2,
        3
    ]

@fmt_off_in_elif
@fmt_off_between_lists
def fmt_off_between_decorators():
    # fmt: skip
    pass
