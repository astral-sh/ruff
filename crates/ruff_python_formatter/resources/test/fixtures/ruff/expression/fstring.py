(
    f'{one}'
    f'{two}'
)


rf"Not-so-tricky \"quote"

# Regression test for fstrings dropping comments
result_f = (
    'Traceback (most recent call last):\n'
    f'  File "{__file__}", line {lineno_f+5}, in _check_recursive_traceback_display\n'
    '    f()\n'
    f'  File "{__file__}", line {lineno_f+1}, in f\n'
    '    f()\n'
    f'  File "{__file__}", line {lineno_f+1}, in f\n'
    '    f()\n'
    f'  File "{__file__}", line {lineno_f+1}, in f\n'
    '    f()\n'
    # XXX: The following line changes depending on whether the tests
    # are run through the interactive interpreter or with -m
    # It also varies depending on the platform (stack size)
    # Fortunately, we don't care about exactness here, so we use regex
    r'  \[Previous line repeated (\d+) more times\]' '\n'
    'RecursionError: maximum recursion depth exceeded\n'
)


# Regression for fstring dropping comments that were accidentally attached to
# an expression inside a formatted value
(
    f'{1}'
    # comment
    ''
)

(
    f'{1}'  # comment
    f'{2}'
)

(
    f'{1}'
    f'{2}'  # comment
)

(
    1, (  # comment
        f'{2}'
    )
)

(
    (
        f'{1}'
        # comment
    ),
    2
)
