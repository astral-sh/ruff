(x for target in iter)
(x async for target in iter)
(x for target in iter if x in y if a and b if c)
(x for target1 in iter1 if x and y for target2 in iter2 if a > b)
(x for target1 in iter1 if x and y async for target2 in iter2 if a > b)

# Named expression
(x := y + 1 for y in z)

# If expression
(x if y else y for y in z)

# Arguments
" ".join(
    sql
    for sql in (
        "LIMIT %d" % limit if limit else None,
        ("OFFSET %d" % offset) if offset else None,
    )
)
