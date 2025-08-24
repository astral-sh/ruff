# Same as `W605_0.py` but using f-strings and t-strings instead.

#: W605:1:10
regex = f'\.png$'

#: W605:2:1
regex = f'''
\.png$
'''

#: W605:2:6
f(
    f'\_'
)

#: W605:4:6
f"""
multi-line
literal
with \_ somewhere
in the middle
"""

#: W605:1:38
value = f'new line\nand invalid escape \_ here'


#: Okay
regex = fr'\.png$'
regex = f'\\.png$'
regex = fr'''
\.png$
'''
regex = fr'''
\\.png$
'''
s = f'\\'
regex = f'\w'  # noqa
regex = f'''
\w
'''  # noqa

regex = f'\\\_'
value = f'\{{1}}'
value = f'\{1}'
value = f'{1:\}'
value = f"{f"\{1}"}"
value = rf"{f"\{1}"}"

# Okay
value = rf'\{{1}}'
value = rf'\{1}'
value = rf'{1:\}'
value = f"{rf"\{1}"}"

# Regression tests for https://github.com/astral-sh/ruff/issues/10434
f"{{}}+-\d"
f"\n{{}}+-\d+"
f"\n{{}}�+-\d+"

# See https://github.com/astral-sh/ruff/issues/11491
total = 10
ok = 7
incomplete = 3
s = f"TOTAL: {total}\nOK: {ok}\INCOMPLETE: {incomplete}\n"

# Debug text (should trigger)
t = f"{'\InHere'=}"



#: W605:1:10
regex = t'\.png$'

#: W605:2:1
regex = t'''
\.png$
'''

#: W605:2:6
f(
    t'\_'
)

#: W605:4:6
t"""
multi-line
literal
with \_ somewhere
in the middle
"""

#: W605:1:38
value = t'new line\nand invalid escape \_ here'


#: Okay
regex = fr'\.png$'
regex = t'\\.png$'
regex = fr'''
\.png$
'''
regex = fr'''
\\.png$
'''
s = t'\\'
regex = t'\w'  # noqa
regex = t'''
\w
'''  # noqa

regex = t'\\\_'
value = t'\{{1}}'
value = t'\{1}'
value = t'{1:\}'
value = t"{t"\{1}"}"
value = rt"{t"\{1}"}"

# Okay
value = rt'\{{1}}'
value = rt'\{1}'
value = rt'{1:\}'
value = t"{rt"\{1}"}"

# Regression tests for https://github.com/astral-sh/ruff/issues/10434
t"{{}}+-\d"
t"\n{{}}+-\d+"
t"\n{{}}�+-\d+"

# See https://github.com/astral-sh/ruff/issues/11491
total = 10
ok = 7
incomplete = 3
s = t"TOTAL: {total}\nOK: {ok}\INCOMPLETE: {incomplete}\n"

# Debug text (should trigger)
t = t"{'\InHere'=}"
