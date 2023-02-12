x = set(x for x in range(3))
x = set(
    x for x in range(3)
)
y = f'{set(a if a < 6 else 0  for a in range(3))}'
_ = '{}'.format(set(a if a < 6 else 0  for a in range(3)))
print(f'Hello {set(a for a in range(3))} World')

def set(*args, **kwargs):
    return None


set(x for x in range(3))
