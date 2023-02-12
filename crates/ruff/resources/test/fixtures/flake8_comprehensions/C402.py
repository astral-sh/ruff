dict((x, x) for x in range(3))
dict(
    (x, x) for x in range(3)
)
dict(((x, x) for x in range(3)), z=3)
y = f'{dict((x, x) for x in range(3))}'
print(f'Hello {dict((x, x) for x in range(3))} World')
