(x for y in z)
(a async for i in iter)
(b for c in d if x in w if y and yy if z)
(a for b in c if d and e for f in j if k > h)
(a for b in c if d and e async for f in j if k > h)
foo(x for i in data)
foo(a, x for i in data)
foo(a, x for i, j in data)

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
