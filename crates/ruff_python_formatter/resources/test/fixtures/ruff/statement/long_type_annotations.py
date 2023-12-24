x1: A[b] | EventHandler | EventSpec | list[EventHandler | EventSpec] | Other | More | AndMore | None = None

x2: "VeryLongClassNameWithAwkwardGenericSubtype[int] |" "VeryLongClassNameWithAwkwardGenericSubtype[str]"

x6: VeryLongClassNameWithAwkwardGenericSubtype[
    integeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeer,
    VeryLongClassNameWithAwkwardGenericSubtype,
    str
] = True


x7: CustomTrainingJob | CustomContainerTrainingJob | CustomPythonPackageTrainingJob
x8: (
    None
    | datasets.ImageDataset
    | datasets.TabularDataset
    | datasets.TextDataset
    | datasets.VideoDataset
) = None

x9: None | (
    datasets.ImageDataset
    | datasets.TabularDataset
    | datasets.TextDataset
    | datasets.VideoDataset
) = None


x10: (
    aaaaaaaaaaaaaaaaaaaaaaaa[
        bbbbbbbbbbb,
        Subscript
        | None
        | datasets.ImageDataset
        | datasets.TabularDataset
        | datasets.TextDataset
        | datasets.VideoDataset,
    ],
    bbb[other],
) = None

x11: None | [
    datasets.ImageDataset,
    datasets.TabularDataset,
    datasets.TextDataset,
    datasets.VideoDataset,
] = None

x12: None | [
        datasets.ImageDataset,
        datasets.TabularDataset,
        datasets.TextDataset,
        datasets.VideoDataset,
    ] | Other = None


x13: [
     datasets.ImageDataset,
     datasets.TabularDataset,
     datasets.TextDataset,
     datasets.VideoDataset,
] | Other = None

x14: [
    datasets.ImageDataset,
    datasets.TabularDataset,
    datasets.TextDataset,
    datasets.VideoDataset,
] | [
     datasets.ImageDataset,
     datasets.TabularDataset,
     datasets.TextDataset,
     datasets.VideoDataset,
] = None

x15: [
    datasets.ImageDataset,
    datasets.TabularDataset,
    datasets.TextDataset,
    datasets.VideoDataset,
] | [
    datasets.ImageDataset,
    datasets.TabularDataset,
    datasets.TextDataset,
    datasets.VideoDataset,
] | Other = None

x16: None | Literal[
    "split",
    "a bit longer",
    "records",
    "index",
    "table",
    "columns",
    "values",
] = None

x17: None | [
    datasets.ImageDataset,
    datasets.TabularDataset,
    datasets.TextDataset,
    datasets.VideoDataset,
]


class Test:
    safe_age: Decimal  #  the user's age, used to determine if it's safe for them to use ruff
    applied_fixes: int  # the number of fixes that this user applied. Used for ranking the users with the most applied fixes.
    string_annotation: "Test"  # a long comment after a quoted, runtime-only type annotation


##########
# Comments

leading: (
    #  Leading comment
    None | dataset.ImageDataset
)

leading_with_value: (
    #  Leading comment
    None
    | dataset.ImageDataset
) = None

leading_open_parentheses: ( #  Leading comment
    None
    | dataset.ImageDataset
)

leading_open_parentheses_with_value: ( #  Leading comment
    None
    | dataset.ImageDataset
) = None

trailing: (
    None | dataset.ImageDataset  # trailing comment
)

trailing_with_value: (
    None | dataset.ImageDataset  # trailing comment
) = None

trailing_own_line: (
    None | dataset.ImageDataset
    #  trailing own line
)

trailing_own_line_with_value: (
    None | dataset.ImageDataset
    #  trailing own line
) = None

nested_comment: None | [
    # a list of strings
    str
] = None
