# The lexer can't be moved back for a triple-quoted f-string because the newlines are
# part of the f-string itself.
# https://github.com/astral-sh/ruff/issues/11937

f'''{foo:.3f
'''