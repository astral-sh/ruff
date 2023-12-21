def foo_brackets(request):
    return JsonResponse(
        {
            "var_1": foo,
            "var_2": bar,
        }
    )

def foo_square_brackets(request):
    return JsonResponse(
        [
            "var_1",
            "var_2",
        ]
    )

func({"a": 37, "b": 42, "c": 927, "aaaaaaaaaaaaaaaaaaaaaaaaa": 11111111111111111111111111111111111111111})

func(["random_string_number_one","random_string_number_two","random_string_number_three","random_string_number_four"])

func(
    {
        # expand me
        'a':37,
        'b':42,
        'c':927
    }
)

func(
    [
        'a',
        'b',
        'c',
    ]
)

func(
    [
        'a',
        'b',
        'c',
    ],
)

func(  # a
    [  # b
        "c",  # c
        "d",  # d
        "e",  # e
    ]  # f
)  # g

func(  # a
    {  # b
        "c": 1,  # c
        "d": 2,  # d
        "e": 3,  # e
    }  # f
)  # g

func(
    # preserve me
    [
        "c",
        "d",
        "e",
    ]
)

func(
    [  # preserve me but hug brackets
        "c",
        "d",
        "e",
    ]
)

func(
    [
        # preserve me but hug brackets
        "c",
        "d",
        "e",
    ]
)

func(
    [
        "c",
        # preserve me but hug brackets
        "d",
        "e",
    ]
)

func(
    [
        "c",
        "d",
        "e",
        # preserve me but hug brackets
    ]
)

func(
    [
        "c",
        "d",
        "e",
    ]  # preserve me but hug brackets
)

func(
    [
        "c",
        "d",
        "e",
    ]
    # preserve me
)

func([x for x in "short line"])
func([x for x in "long line long line long line long line long line long line long line"])
func([x for x in [x for x in "long line long line long line long line long line long line long line"]])

foooooooooooooooooooo(
    [{c: n + 1 for c in range(256)} for n in range(100)] + [{}], {size}
)

baaaaaaaaaaaaar(
    [1, 2, 3, 4, 5, 6, 7, 8, 9, 10], {x}, "a string", [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
)

nested_mapping = {"key": [{"a very long key 1": "with a very long value", "a very long key 2": "with a very long value"}]}
explicit_exploding = [[["short", "line",],],]
single_item_do_not_explode = Context({
    "version": get_docs_version(),
})

foo(*[str(i) for i in range(100000000000000000000000000000000000000000000000000000000000)])

foo(
    **{
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa": 1,
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb": 2,
        "ccccccccccccccccccccccccccccccccc": 3,
        **other,
    }
)

foo(**{x: y for x, y in enumerate(["long long long long line","long long long long line"])})

# Edge case when deciding whether to hug the brackets without inner content.
very_very_very_long_variable = very_very_very_long_module.VeryVeryVeryVeryLongClassName([[]])

for foo in ["a", "b"]:
    output.extend([
        individual
        for
        # Foobar
        container in xs_by_y[foo]
        # Foobar
        for individual in container["nested"]
    ])
