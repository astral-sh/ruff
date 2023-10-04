
# No Ternary (No Error)

1<2 and 'b' and 'c'

1<2 or 'a' and 'b'

1<2 and 'a'

1<2 or 'a'

2>1

# Ternary (Error)

1<2 and 'a' or 'b'

1<2 and 'a' or 'b' and 'c'

1<2 and 'a' or 'b' or 'c'

1<2 and 'a' or 'b' or 'c' or (lambda x: x+1)

1<2 and 'a' or 'b' or (lambda x: x+1) or 'c'

(lambda x: x+1) and 'a' or 'b'

'a' and (lambda x: x+1) or 'orange'

def has_oranges(oranges, apples=None) -> bool:
    return apples and False or oranges  # [simplify-boolean-expression]

a = 1
b = 2
c = 3
a and b or c
