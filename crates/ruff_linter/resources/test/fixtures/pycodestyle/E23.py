#: E231
a = (1,2)
#: E231
a[b1,:]
#: E231
a = [{'a':''}]
#: Okay
a = (4,)
b = (5, )
c = {'text': text[5:]}

result = {
    'key1': 'value',
    'key2': 'value',
}

def foo() -> None:
    #: E231
    if (1,2):
        pass

#: Okay
a = (1,\
2)

#: E231:2:20
mdtypes_template = {
    'tag_full': [('mdtype', 'u4'), ('byte_count', 'u4')],
    'tag_smalldata':[('byte_count_mdtype', 'u4'), ('data', 'S4')],
}

# E231
f"{(a,b)}"

# Okay because it's hard to differentiate between the usages of a colon in a f-string
f"{a:=1}"
f"{ {'a':1} }"
f"{a:.3f}"
f"{(a:=1)}"
f"{(lambda x:x)}"
f"normal{f"{a:.3f}"}normal"

#: Okay
snapshot.file_uri[len(f's3://{self.s3_bucket_name}/'):]

#: E231
{len(f's3://{self.s3_bucket_name}/'):1}

#: Okay
a = (1,)


# https://github.com/astral-sh/ruff/issues/10113
"""Minimal repo."""

def main() -> None:
    """Primary function."""
    results = {
        "k1": [1],
        "k2":[2],
    }
    results_in_tuple = (
        {
            "k1": [1],
            "k2":[2],
        },
    )
    results_in_list = [
        {
            "k1": [1],
            "k2":[2],
        }
    ]
    results_in_list_first = [
        {
            "k2":[2],
        }
    ]

x = [
    {
        "k1":[2], # E231
        "k2": x[2:4],
        "k3":[2],  # E231
        "k4": [2],
        "k5": [2],
        "k6": [1, 2, 3, 4,5,6,7]  # E231
    },
    {
        "k1": [
            {
                "ka":[2,3],  # E231
            },
            {
                "kb": [2,3],  # E231
            },
            {
                "ka":[2, 3],  # E231
                "kb": [2, 3],  # Ok
                "kc": [2, 3],  # Ok
                "kd": [2,3],  # E231
                "ke":[2,3],  # E231
            },
        ]
    }
]

# Should be E231 errors on all of these type parameters and function parameters, but not on their (strange) defaults
def pep_696_bad[A:object="foo"[::-1], B:object =[[["foo", "bar"]]], C:object= bytes](
    x:A = "foo"[::-1],
    y:B = [[["foo", "bar"]]],
    z:object = "fooo",
):
    pass

class PEP696Bad[A:object="foo"[::-1], B:object =[[["foo", "bar"]]], C:object= bytes]:
    def pep_696_bad_method[A:object="foo"[::-1], B:object =[[["foo", "bar"]]], C:object= bytes](
        self,
        x:A = "foo"[::-1],
        y:B = [[["foo", "bar"]]],
        z:object = "fooo",
    ):
        pass

class PEP696BadWithEmptyBases[A:object="foo"[::-1], B:object =[[["foo", "bar"]]], C:object= bytes]():
    class IndentedPEP696BadWithNonEmptyBases[A:object="foo"[::-1], B:object =[[["foo", "bar"]]], C:object= bytes](object, something_dynamic[x::-1]):
        pass

# Should be no E231 errors on any of these:
def pep_696_good[A: object="foo"[::-1], B: object =[[["foo", "bar"]]], C: object= bytes](
    x: A = "foo"[::-1],
    y: B = [[["foo", "bar"]]],
    z: object = "fooo",
):
    pass

class PEP696Good[A: object="foo"[::-1], B: object =[[["foo", "bar"]]], C: object= bytes]:
    pass

class PEP696GoodWithEmptyBases[A: object="foo"[::-1], B: object =[[["foo", "bar"]]], C: object= bytes]():
    pass

class PEP696GoodWithNonEmptyBases[A: object="foo"[::-1], B: object =[[["foo", "bar"]]], C: object= bytes](object, something_dynamic[x::-1]):
    pass

# E231
t"{(a,b)}"

# Okay because it's hard to differentiate between the usages of a colon in a t-string
t"{a:=1}"
t"{ {'a':1} }"
t"{a:.3f}"
t"{(a:=1)}"
t"{(lambda x:x)}"
t"normal{t"{a:.3f}"}normal"

#: Okay
snapshot.file_uri[len(t's3://{self.s3_bucket_name}/'):]

#: E231
{len(t's3://{self.s3_bucket_name}/'):1}
