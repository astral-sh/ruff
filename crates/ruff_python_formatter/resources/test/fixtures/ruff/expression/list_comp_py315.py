[*x for x in y]

[
    *  # comment between * and x
    x
    for x in y
]

[
    *values
    for values in some_really_long_collection_name_that_should_force_wrapping
]

{*x for x in y}

{
    *  # comment between * and x
    x
    for x in y
}

{**d for d in dicts}

{
    **d
    for d in dicts
}

{
    **  # comment between ** and d
    d
    for d in dicts
}

(*x for x in y)

f(*x for x in y)
