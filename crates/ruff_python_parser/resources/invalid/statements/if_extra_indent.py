# Improving the recovery would require changing the lexer to emit an extra dedent token after `a + b`.
if True:
    pass
        a + b

    pass

a = 10
