# Recursive binary literal widening convergence

This is minimized from a Vision ecosystem failure. A loop-carried numeric accumulator can be
combined with both literal arithmetic and an unknown operand. The literal arithmetic branch used to
keep producing one more exact integer literal on each fixed-point iteration and panic with "too many
cycle iterations".

```py
def plain_binary(mode, trials, value):
    counts = 0

    for _ in range(trials):
        if mode:
            counts = counts + 1
        else:
            counts = counts + value

    return counts

def augmented_assignment(mode, trials, value):
    counts = 0

    for _ in range(trials):
        if mode:
            counts += 1
        else:
            counts += value

    return counts
```
