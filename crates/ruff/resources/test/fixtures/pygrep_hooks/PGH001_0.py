from ast import literal_eval

eval("3 + 4")

literal_eval({1: 2})


def fn() -> None:
    eval("3 + 4")
