dict((x, x) for x in range(3))
dict(
    (x, x) for x in range(3)
)
dict(((x, x) for x in range(3)), z=3)
y = f'{dict((x, x) for x in range(3))}'
print(f'Hello {dict((x, x) for x in range(3))} World')
print(f"Hello {dict((x, x) for x in 'abc')} World")
print(f'Hello {dict((x, x) for x in "abc")} World')
print(f'Hello {dict((x,x) for x in "abc")} World')

f'{dict((x, x) for x in range(3)) | dict((x, x) for x in range(3))}'
f'{ dict((x, x) for x in range(3)) | dict((x, x) for x in range(3)) }'

def f(x):
    return x

print(f'Hello {dict((x,f(x)) for x in "abc")} World')

# Regression test for: https://github.com/astral-sh/ruff/issues/7086
dict((k,v)for k,v in d.iteritems() if k in only_args)

# Regression test for: https://github.com/astral-sh/ruff/issues/7455#issuecomment-1722458940
dict((*v, k) for k, v in enumerate(calendar.month_abbr))
