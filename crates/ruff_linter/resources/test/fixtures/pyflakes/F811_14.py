try:
    import b
except ImportError:
    b = Ellipsis
    from bb import a
else:
    from aa import a
finally:
    a = 42
print(a, b)
