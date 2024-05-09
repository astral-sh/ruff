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

# OK
QUERY_EXECUTION_COUNT_SQL = f"""
SELECT day_start, sum(total) AS total FROM (
    SELECT
        0 AS total,
        toStartOfDay(now() - toIntervalDay(number)) AS day_start
    FROM numbers(dateDiff('day', toStartOfDay(now()  - INTERVAL %(days)s day), now()))
    UNION ALL
    SELECT
        count(*) AS total,
        toStartOfDay(query_start_time) AS day_start
    FROM
        {QUERY_LOG_SYSTEM_TABLE}
    WHERE
        query_start_time > now() - INTERVAL %(days)s day AND type = 2 AND is_initial_query %(conditions)s
    GROUP BY day_start
)
GROUP BY day_start
ORDER BY day_start asc
"""
