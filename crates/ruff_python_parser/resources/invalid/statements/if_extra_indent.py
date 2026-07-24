# Improving the recovery would require changing the lexer to emit an extra dedent token after `a + b`.
if True:
    pass
        a + b

    pass

a = 10

# Multiple nested unexpected indents.
if True:
    before_nested
        first_nested
            second_nested
    after_nested

outside_nested

# A valid compound statement inside recovered indentation.
if True:
    before_compound
        if condition:
            nested_compound
        recovered_compound
    after_compound

outside_compound

# Multiple independent unexpected-indent regions in the same body.
if True:
    before_regions
        first_region
    middle_region
        second_region
    after_region

outside_regions

# An independent syntax error inside recovered indentation stays visible.
if True:
    before_error
        broken(,)
    after_error

outside_error

# Outstanding unexpected indents are flushed at EOF.
if True:
    before_eof
        final_eof
