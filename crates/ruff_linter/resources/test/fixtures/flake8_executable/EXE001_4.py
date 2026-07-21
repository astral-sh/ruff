# `#!` comments that are not shebangs (no interpreter path). None of these should
# trigger any EXE rule.


def f():
    #! not a shebang — just a comment
    return 1


x = 1  #! inline pseudo-shebang

#!python  # no slash — not a valid shebang on Linux

#!  # empty — not a shebang
