# The newline character is being escaped which means that the lexer shouldn't be moved
# back to that position.
# https://github.com/astral-sh/ruff/issues/12004

f'middle {'string':\
        'format spec'}

f'middle {'string':\\
        'format spec'}

f'middle {'string':\\\
        'format spec'}