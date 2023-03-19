x = [1, 2, 3]
y = [("a", 1), ("b", 2), ("c", 3)]

[i for i in x]
{i for i in x}
{k: v for k, v in y}

[i for i in x if i > 1]
[i for i in x for j in x]
{v: k for k, v in y}
