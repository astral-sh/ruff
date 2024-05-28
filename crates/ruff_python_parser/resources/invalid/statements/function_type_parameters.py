# FIXME: The type param related error message and the parser recovery are looking pretty good **except**
# that the lexer never recovers from the unclosed `[`, resulting in it lexing `NonLogicalNewline` tokens instead of `Newline` tokens.
# That's because the parser has no way of feeding the error recovery back to the lexer,
# so they don't agree on the state of the world which can lead to all kind of errors further down in the file.
# This is not just a problem with parentheses but also with the transformation made by the
# `SoftKeywordTransformer` because the `Parser` and `Transformer` may not agree if they're
# currently in a position where the `type` keyword is allowed or not.
# That roughly means that any kind of recovery can lead to unrelated syntax errors
# on following lines.

def keyword[A, await](): ...

def not_a_type_param[A, |, B](): ...

def multiple_commas[A,,B](): ...

def multiple_trailing_commas[A,,](): ...

def multiple_commas_and_recovery[A,,100](): ...
