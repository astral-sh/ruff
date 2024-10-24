# Preview style: hug brackets to call parentheses.
func([1, 2, 3,])

func(  # comment
[1, 2, 3,])

func(
    # comment
[1, 2, 3,])

func([1, 2, 3,]  # comment
)

func([1, 2, 3,]
    # comment
)

func([  # comment
    1, 2, 3,]
)

func(([1, 2, 3,]))


func(
    (
        1,
        2,
        3,
    )
)

# Ensure that comprehensions hug too.
func([(x, y,) for (x, y) in z])

# Ensure that dictionaries hug too.
func({1: 2, 3: 4, 5: 6,})

# Ensure that the same behavior is applied to parenthesized expressions.
([1, 2, 3,])

( # comment
    [1, 2, 3,])

(
    [  # comment
    1, 2, 3,])

# Ensure that starred arguments are also hugged.
foo(
    *[
        a_long_function_name(a_long_variable_name)
        for a_long_variable_name in some_generator
    ]
)

foo(
    *  # comment
    [
        a_long_function_name(a_long_variable_name)
        for a_long_variable_name in some_generator
    ]
)

foo(
    **[
        a_long_function_name(a_long_variable_name)
        for a_long_variable_name in some_generator
    ]
)

foo(
    **  # comment
    [
        a_long_function_name(a_long_variable_name)
        for a_long_variable_name in some_generator
    ]
)

# Ensure that multi-argument calls are _not_ hugged.
func([1, 2, 3,], bar)

func([(x, y,) for (x, y) in z], bar)


# Ensure that nested lists are hugged.
func([
    [
        1,
        2,
        3,
    ]
])


func([
    # comment
    [
        1,
        2,
        3,
    ]
])

func([
    [
        1,
        2,
        3,
    ]
    # comment
])

func([
    [  # comment
        1,
        2,
        3,
    ]
])


func([  # comment
    [
        1,
        2,
        3,
    ]
])
