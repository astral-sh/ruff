score: int = 73
passing: bool = score >= 60
excellent: bool = score >= 90

if excellent:
    print(2)
elif passing:
    print(1)
else:
    print(0)
