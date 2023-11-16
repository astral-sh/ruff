def f():
    # fmt: off
    a = 10

    if True:
        with_semicolon = 10   \
            ;

formatted   = true;


def f():
    # fmt: off

    if True:
        with_semicolon = 20 \
            ; # comment


# fmt: off
statement = 0 \
    ;
# fmt: on
a = 10

# fmt: off
last_statement_with_semi   ;
