foo(**{"bar": True})  # PIE804

foo(**{"r2d2": True})  # PIE804

Foo.objects.create(**{"bar": True})  # PIE804

Foo.objects.create(**{"_id": some_id})  # PIE804

Foo.objects.create(**{**bar})  # PIE804

foo(**{})

foo(**{**data, "foo": "buzz"})
foo(**buzz)
foo(**{"bar-foo": True})
foo(**{"bar foo": True})
foo(**{"1foo": True})
foo(**{buzz: True})
foo(**{"": True})
foo(**{f"buzz__{bar}": True})
abc(**{"for": 3})
foo(**{},)

# Duplicated key names won't be fixed, to avoid syntax errors.
abc(**{'a': b}, **{'a': c})  # PIE804
abc(a=1, **{'a': c}, **{'b': c})  # PIE804

# Some values need to be parenthesized.
abc(foo=1, **{'bar': (bar := 1)})  # PIE804
abc(foo=1, **{'bar': (yield 1)})  # PIE804
