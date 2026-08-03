# `#!` comments that are not at the start of the file are not treated as
# shebangs and must not trigger `EXE001`.


def f():
    #! not a shebang — just a comment
    return 1


x = 1  #! inline pseudo-shebang

#!python

#!
