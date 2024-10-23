# No indentation before the function definition
if call(foo
def bar():
    pass


# Indented function definition
if call(foo
    def bar():
        pass


# There are multiple non-logical newlines (blank lines) in the `if` body
if call(foo


    def bar():
        pass


# There are trailing whitespaces in the blank line inside the `if` body
if call(foo
        
    def bar():
        pass


# The lexer is nested with multiple levels of parentheses
if call(foo, [a, b
    def bar():
        pass


# The outer parenthesis is closed but the inner bracket isn't
if call(foo, [a, b)
    def bar():
        pass


# The parser tries to recover from an unclosed `]` when the current token is `)`. This
# test is to make sure it emits a `NonLogicalNewline` token after `b`.
if call(foo, [a,
    b
)
    def bar():
        pass


# F-strings uses normal list parsing, so test those as well
if call(f"hello {x
    def bar():
        pass


if call(f"hello
    def bar():
        pass