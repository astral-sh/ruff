x = [1, 2, 3]
y = [("a", 1), ("b", 2), ("c", 3)]
z = [(1,), (2,), (3,)]
d = {"a": 1, "b": 2, "c": 3}

[i for i in x]
{i for i in x}
{k: v for k, v in y}
{k: v for k, v in d.items()}
[(k, v) for k, v in d.items()]
{k: (a, b) for k, (a, b) in d.items()}

[i for i, in z]
[i for i, j in y]
[i for i in x if i > 1]
[i for i in x for j in x]

{v: k for k, v in y}
{k.foo: k for k in y}
{k["foo"]: k for k in y}
{k: v if v else None for k, v in y}

# Regression test for: https://github.com/astral-sh/ruff/issues/7196
any(len(symbol_table.get_by_type(symbol_type)) > 0 for symbol_type in[t for t in SymbolType])
