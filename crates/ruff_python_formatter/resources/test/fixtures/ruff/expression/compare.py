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

# comments

c = (
    1 > # 1
    # 2
    3 # 3
    > # 4
    5 # 5
    # 6
)

# Implicit strings and comments

assert (
    "One or more packages has an associated PGP signature; these will "
    "be silently ignored by the index"
    in caplog.messages
)

(
    b < c > d <
    f"aaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    "cccccccccccccccccccccccccc"
    % aaaaaaaaaaaa
    > x
)

c = (a >
     # test leading binary comment
     "a" "b" * b
     )

c = (a *
     # test leading comment
     "a" "b" > b
     )

c = (a
     > # test trailing comment
     "a" "b" * b
     )

c = (a
     >
     "a" "b" # test trailing comment
     * b
     )

c = (a
     >
     "a" "b" # test trailing binary comment
     + b
     )


c = (a >
     # test leading binary comment
     "a" "b" * b
     )

c = (a *
     # test leading comment
     "a" "b" > b
     )

