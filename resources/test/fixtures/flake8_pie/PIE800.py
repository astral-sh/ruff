{**foo,    "bar": True  }
{"foo": 1, **{"bar": 1}}  # PIE800

foo({**foo, **{"bar": True}})  # PIE800

{**foo, **{"bar": 10}}  # PIE800

{**foo, **buzz, **{bar: 10}}  # PIE800

{"foo": 1, "buzz": {"bar": 1}} # okay

{**foo,    "bar": True  } # okay

Table.objects.filter(inst=inst, **{f"foo__{bar}__exists": True}) # okay

buzz = {**foo, "bar": { 1: 2 }} # okay
