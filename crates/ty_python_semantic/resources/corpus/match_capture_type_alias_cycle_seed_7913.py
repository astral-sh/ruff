# Regression test for a cycle found with py-fuzzer seed 7913.

type Alias = captured
{Alias}
match missing or (lambda: flag):
    case 1 if lambda *, flag: flag:
        pass
    case {0: {**captured}, **other} if flag := captured:
        pass
