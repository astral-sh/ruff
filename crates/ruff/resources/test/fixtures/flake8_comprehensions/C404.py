dict([(i, i) for i in range(3)])
dict([(i, i) for i in range(3)], z=4)

def f(x):
    return x

f'{dict([(s,s) for s in "ab"])}'
f"{dict([(s,s) for s in 'ab'])}"
f"{dict([(s, s) for s in 'ab'])}"
f"{dict([(s,f(s)) for s in 'ab'])}"

f'{dict([(s,s) for s in "ab"]) | dict([(s,s) for s in "ab"])}'
f'{ dict([(s,s) for s in "ab"]) | dict([(s,s) for s in "ab"]) }'
