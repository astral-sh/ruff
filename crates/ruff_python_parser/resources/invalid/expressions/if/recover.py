# Invalid test expression
x if *expr else y
x if lambda x: x else y
x if yield x else y
x if yield from x else y

# Invalid orelse expression
x if expr else *orelse
x if expr else yield y
x if expr else yield from y