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

# Regression test for: https://github.com/astral-sh/ruff/issues/7087
saved.append(dict([(k, v)for k,v in list(unique_instance.__dict__.items()) if k in [f.name for f in unique_instance._meta.fields]]))
