a == b
a != b
a < b
a <= b
a > b
a >= b
a is b
a is not b
a in b
a not in b

(a ==
  # comment
    b
)

(a == # comment
 b
 )

a < b > c == d

aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa < bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb > ccccccccccccccccccccccccccccc == ddddddddddddddddddddd

aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa < [
    bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,
    ff,
] < [ccccccccccccccccccccccccccccc, dddd] < ddddddddddddddddddddddddddddddddddddddddddd

return 1 == 2 and (
    name,
    description,
    self_default,
    self_selected,
    self_auto_generated,
    self_parameters,
    self_meta_data,
    self_schedule,
) == (
    name,
    description,
    othr_default,
    othr_selected,
    othr_auto_generated,
    othr_parameters,
    othr_meta_data,
    othr_schedule,
)

(name, description, self_default, self_selected, self_auto_generated, self_parameters, self_meta_data, self_schedule) == (name, description, other_default, othr_selected, othr_auto_generated, othr_parameters, othr_meta_data, othr_schedule)
((name, description, self_default, self_selected, self_auto_generated, self_parameters, self_meta_data, self_schedule) == (name, description, other_default, othr_selected, othr_auto_generated, othr_parameters, othr_meta_data, othr_schedule))

[
    (
        a
        + [
            bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
        ]
        >= c
    )
]

def f():
    return (
        unicodedata.normalize("NFKC", s1).casefold()
        == unicodedata.normalize("NFKC", s2).casefold()
    )

def f():
    return (
        unicodedata.normalize("NFKC", s1).casefold(x)
        == unicodedata.normalize("NFKC", s2).casefold(x)
    )

def f():
    return (
        unicodedata.normalize("NFKC", s1).casefold(x)
        == unicodedata.normalize("NFKC", s2).casefold()
    )

def f():
    return (
        unicodedata.normalize("NFKC", s1).casefold()
        == unicodedata.normalize("NFKC", s2).casefold(x)
    )

# Call expressions with trailing attributes.

ct_match = (
    aaaaaaaaaaact_id == self.get_content_type(obj=rel_obj, using=instance._state.db).id
)

ct_match = (
    {aaaaaaaaaaaaaaaa} == self.get_content_type(obj=rel_obj, using=instance._state.db).id
)

ct_match = (
    (aaaaaaaaaaaaaaaa) == self.get_content_type(obj=rel_obj, using=instance._state.db).id
)

ct_match = aaaaaaaaaaact_id == self.get_content_type(
    obj=rel_obj, using=instance._state.db
)

# Call expressions with trailing subscripts.

ct_match = (
    aaaaaaaaaaact_id == self.get_content_type(obj=rel_obj, using=instance._state.db)[id]
)

ct_match = (
    {aaaaaaaaaaaaaaaa} == self.get_content_type(obj=rel_obj, using=instance._state.db)[id]
)

ct_match = (
    (aaaaaaaaaaaaaaaa) == self.get_content_type(obj=rel_obj, using=instance._state.db)[id]
)

# Subscripts expressions with trailing attributes.

ct_match = (
    aaaaaaaaaaact_id == self.get_content_type[obj, rel_obj, using, instance._state.db].id
)

ct_match = (
    {aaaaaaaaaaaaaaaa} == self.get_content_type[obj, rel_obj, using, instance._state.db].id
)

ct_match = (
    (aaaaaaaaaaaaaaaa) == self.get_content_type[obj, rel_obj, using, instance._state.db].id
)

# Expressions with empty parentheses.
ct_match = (
    unicodedata.normalize("NFKC", s1).casefold()
    == unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    unicodedata.normalize("NFKC", s1).casefold(
        # foo
    )
    == unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold(
        # foo
    )
)

ct_match = (
    unicodedata.normalize("NFKC", s1).casefold(x)
    == unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold(x)
)

ct_match = (
    unicodedata.normalize("NFKC", s1).casefold(x)
    == unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    unicodedata.normalize("NFKC", s1).casefold()
    == unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold(x)
)

ct_match = (
    [].unicodedata.normalize("NFKC", s1).casefold()
    == [].unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    [1].unicodedata.normalize("NFKC", s1).casefold()
    == [].unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    [].unicodedata.normalize("NFKC", s1).casefold()
    == [1].unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    [1].unicodedata.normalize("NFKC", s1).casefold()
    == [1].unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    [1,].unicodedata.normalize("NFKC", s1).casefold()
    == [].unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    [ # comment
    ].unicodedata.normalize("NFKC", s1).casefold()
    == [].unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    unicodedata().normalize("NFKC", s1).casefold()
    == unicodedata().normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    unicodedata(1).normalize("NFKC", s1).casefold()
    == unicodedata().normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    unicodedata().normalize("NFKC", s1).casefold()
    == unicodedata(1).normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    unicodedata(1).normalize("NFKC", s1).casefold()
    == unicodedata(1).normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    [1, 2,].unicodedata.normalize("NFKC", s1).casefold()
    == [].unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    foo(1, 2,).unicodedata.normalize("NFKC", s1).casefold()
    == [].unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    foo[1, 2,].unicodedata.normalize("NFKC", s1).casefold()
    == [].unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    unicodedata.normalize("NFKC", s1).casefold()
    == unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold(1, 2)
)

ct_match = (
    unicodedata.normalize("NFKC", s1).casefold()
    == unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold[1, 2,]
)
