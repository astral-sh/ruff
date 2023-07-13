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
