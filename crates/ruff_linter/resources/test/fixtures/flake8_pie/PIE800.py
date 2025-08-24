{"foo": 1, **{"bar": 1}}  # PIE800

{**{"bar": 10}, "a": "b"}  # PIE800

foo({**foo, **{"bar": True}})  # PIE800

{**foo, **{"bar": 10}}  # PIE800

{ # PIE800
    "a": "b",
    # Preserve
    **{
        # all
        "bar": 10,  # the
        # comments
    },
}

{**foo, **buzz, **{bar: 10}}  # PIE800

# https://github.com/astral-sh/ruff/issues/15366
{
    "data": [],
    **({"count": 1 if include_count else {}}),
}

{
    "data": [],
    **(  # Comment
        {  # Comment
            "count": 1 if include_count else {}}),
}

{
    "data": [],
    **(
        {
            "count": (a := 1),}),
}

{
    "data": [],
    **(
        {
            "count": (a := 1)
            }
    )
    ,
}

{
    "data": [],
    **(
        {
            "count": (a := 1),  # Comment
            }  # Comment
    ) # Comment
    ,
}

({
    "data": [],
    **(  # Comment
        (   # Comment
{ # Comment
            "count": (a := 1),  # Comment
        }  # Comment
            )
    ) # Comment
    ,
})

{
    "data": [],
    ** # Foo
    (  # Comment
        { "a": b,
          # Comment
          }
    ) ,
    c: 9,
}


# https://github.com/astral-sh/ruff/issues/15997
{"a": [], **{},}
{"a": [], **({}),}

{"a": [], **{}, 6: 3}
{"a": [], **({}), 6: 3}

{"a": [], **{
    # Comment
}, 6: 3}
{"a": [], **({
    # Comment
}), 6: 3}


{**foo,    "bar": True  }  # OK

{"foo": 1, "buzz": {"bar": 1}}  # OK

{**foo,    "bar": True  }  # OK

Table.objects.filter(inst=inst, **{f"foo__{bar}__exists": True})  # OK

buzz = {**foo, "bar": { 1: 2 }}  # OK
