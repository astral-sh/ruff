#: E122
print("E122", (
"str"))

# OK
print("E122", (
    "str"))

#: E122:6:5 E122:7:5 E122:8:1
print(dedent(
    '''
        mkdir -p ./{build}/
        mv ./build/ ./{build}/%(revision)s/
    '''.format(
    build='build',
    # more stuff
)
))

# OK
print(dedent(
    '''
        mkdir -p ./{build}/
        mv ./build/ ./{build}/%(revision)s/
    '''.format(
        build='build',
        # more stuff
    )
))

#: E122
if True:
    result = some_function_that_takes_arguments(
        'a', 'b', 'c',
        'd', 'e', 'f',
)

# OK
if True:
    result = some_function_that_takes_arguments(
        'a', 'b', 'c',
        'd', 'e', 'f',
    )

#: E122
if some_very_very_very_long_variable_name or var \
or another_very_long_variable_name:
    raise Exception()

# OK
if some_very_very_very_long_variable_name or var \
    or another_very_long_variable_name:
    raise Exception()

#: E122
if some_very_very_very_long_variable_name or var[0] \
or another_very_long_variable_name:
    raise Exception()

# OK
if some_very_very_very_long_variable_name or var[0] \
    or another_very_long_variable_name:
    raise Exception()

#: E122
if True:
    if some_very_very_very_long_variable_name or var \
    or another_very_long_variable_name:
        raise Exception()

# OK
if True:
    if some_very_very_very_long_variable_name or var \
        or another_very_long_variable_name:
        raise Exception()

#: E122
if True:
    if some_very_very_very_long_variable_name or var[0] \
    or another_very_long_variable_name:
        raise Exception()

#: OK
if True:
    if some_very_very_very_long_variable_name or var[0] \
        or another_very_long_variable_name:
        raise Exception()

#: E122
dictionary = {
    "is": {
    "nested": yes(),
    },
}

# OK
dictionary = {
    "is": {
        "nested": yes(),
    },
}

#: E122
setup('',
    scripts=[''],
    classifiers=[
    'Development Status :: 4 - Beta',
        'Environment :: Console',
        'Intended Audience :: Developers',
    ])

# OK
setup('',
    scripts=[''],
    classifiers=[
        'Development Status :: 4 - Beta',
        'Environment :: Console',
        'Intended Audience :: Developers',
    ])

#: E122:2:1
if True:\
print(True)

# OK
if True:\
    print(True)

# E122
def f():
    x = ((
        (
        )
    )
    + 2)

# OK
def f():
    x = ((
        (
        )
    )
        + 2)

# OK
def target(
    self,
) -> Union[
    Guild,
    Member,
    User,
]:
    ...

# OK
x = [
    [
        1
    ], [
        1
    ]
]
