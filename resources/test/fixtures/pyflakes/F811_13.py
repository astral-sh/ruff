try:
    import funca
except ImportError:
    from bb import funca
    from bb import funcb
else:
    from bbb import funcb
print(funca, funcb)
