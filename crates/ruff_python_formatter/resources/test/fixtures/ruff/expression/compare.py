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
