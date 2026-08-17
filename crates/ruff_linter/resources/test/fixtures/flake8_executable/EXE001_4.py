# `#!` inside a function body is an ordinary comment, not a shebang.
def f():
    #! regular comment
    return 1
