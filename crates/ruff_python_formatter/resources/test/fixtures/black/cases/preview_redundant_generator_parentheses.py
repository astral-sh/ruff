any((x for x in items))
service.consume((item for group in groups for item in group if item))
factory((item for item in iterable))()

((item.is_valid() for item in items))
(((item.is_valid() for item in items)),)
[((item.is_valid() for item in items)), ""]
{"foo": ((item.is_valid() for item in items))}
outer(inner(((item.is_valid() for item in items))))
((result := transform(item) for item in items))
((item async for item in stream()))
next(((item for item in items)), None)
consume(generator=((item for item in items)))
consume(*((item for item in items)))

# Power trailers still require the generator's own parentheses.
((item for item in items)).send(None)
((item for item in items))[0]
((item for item in items)).attribute
((item for item in items))()
(item for item in items).send(None)
(item for item in items)[0]
(item for item in items).attribute
(item for item in items)()

any((
    this_very_long_method_name(obj) or another_long_method_name(obj)
    for obj in this_iterable
))

# Every layer of redundant parentheses goes, including when the generator splits.
outer(inner((((item for item in items)))))
outer(inner(((item.is_valid_and_ready(runtime_context) for item in the_collection_of_items_to_check))))

# Existing comment handling may keep the generator parentheses visible.
any((
    long_expression_that_exceeds_the_configured_line_length_limit_value  # inline comment
    for item in iterable
))

# Parentheses are required when the generator is not the sole argument.
next((item for item in iterable), None)
enumerate((item for item in iterable), start=10)
consume((item for item in iterable),)
consume(*(item for item in iterable))
consume(generator=(item for item in iterable))
