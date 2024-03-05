def fmt_off_between_lists():
    test_list = [
        # fmt: off
        1,
        2,
        3,
    ]


# note: the second `fmt: skip`` should be OK
def fmt_skip_on_own_line():
    # fmt: skip
    pass  # fmt: skip


@fmt_skip_on_own_line
# fmt: off
@fmt_off_between_lists
def fmt_off_between_decorators():
    pass


@fmt_off_between_decorators
# fmt: off
class FmtOffBetweenClassDecorators:
    ...


def fmt_off_in_else():
    x = [1, 2, 3]
    for val in x:
        print(x)
    # fmt: off
    else:
        print("done")
    while False:
        print("while")
        # fmt: off
    # fmt: off
    else:
        print("done")
    if len(x) > 3:
        print("huh?")
    # fmt: on
    # fmt: off
    else:
        print("expected")


class Test:
    @classmethod
    # fmt: off
    def cls_method_a(
        # fmt: off
        cls,
    ) -> None: # noqa: test # fmt: skip
        pass


def fmt_on_trailing():
    # fmt: off
    val = 5 # fmt: on
    pass # fmt: on


# all of these should be fine
def match_case_and_elif():
    string = "hello"
    match string:
        case ("C"
            | "CX"
            | "R"
            | "RX"
            | "S"
            | "SP"
            | "WAP"
            | "XX"
            | "Y"
            | "YY"
            | "YZ"
            | "Z"
            | "ZZ"
        ):  # fmt: skip
            pass
        case _: # fmt: skip
            if string != "Hello":
                pass
            elif string == "Hello": # fmt: skip
                pass
