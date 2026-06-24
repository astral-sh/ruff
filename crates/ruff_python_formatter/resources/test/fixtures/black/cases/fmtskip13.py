t = (
    {"foo": "very long string", "bar": "another very long string", "baz": "we should run out of space by now"},  # fmt: skip
    {"foo": "bar"},
)

t = (
    {
        "foo": "very long string",
        "bar": "another very long string",
        "baz": "we should run out of space by now",
    },  # fmt: skip
    {"foo": "bar"},
)


t = (
    {"foo": "very long string", "bar": "another very long string", "baz": "we should run out of space by now"},  # fmt: skip
    {"foo": "bar",},
)

t = (
    {
        "foo": "very long string",
        "bar": "another very long string",
        "baz": "we should run out of space by now",
    },  # fmt: skip
    {"foo": "bar",},
)
