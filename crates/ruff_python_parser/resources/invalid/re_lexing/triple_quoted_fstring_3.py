# Here, the nesting level is 2 when the parser is trying to recover from an unclosed `{`
# This test demonstrates that we need to reduce the nesting level when recovering from
# within an f-string but the lexer shouldn't go back.

if call(f'''{x:.3f
'''
    pass