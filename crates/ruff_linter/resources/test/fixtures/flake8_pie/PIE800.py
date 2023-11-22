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

{**foo,    "bar": True  }  # OK

{"foo": 1, "buzz": {"bar": 1}}  # OK

{**foo,    "bar": True  }  # OK

Table.objects.filter(inst=inst, **{f"foo__{bar}__exists": True})  # OK

buzz = {**foo, "bar": { 1: 2 }}  # OK
