url = "www.example.com"

# Errors
prefix = url.split(".")[0]  # [missing-maxsplit-arg]
suffix = url.split(".")[-1]  # [missing-maxsplit-arg]

prefix = "www.example.com".split(".")[0]  # [missing-maxsplit-arg]
suffix = "www.example.com".split(".")[-1]  # [missing-maxsplit-arg]

# OK
prefix = url.split(".", maxsplit=1)[0]
prefix = url.split(".", 1)[0]
suffix = url.rsplit(".", maxsplit=1)[-1]
suffix = url.rsplit(".", 1)[-1]

prefix = "www.example.com".split(".", maxsplit=1)[0]
prefix = "www.example.com".split(".", 1)[0]
suffix = "www.example.com".rsplit(".", maxsplit=1)[-1]
suffix = "www.example.com".rsplit(".", 1)[-1]

# OK - not called on str.split
any_expr_with_slice = "asdf"[0]
any_func_with_slice = list("asdf")[0]

class Splitter(str):
    def split(self, sep=None, maxsplit=-1):
        return super().split(sep, maxsplit)

user_defined_split = Splitter(url).split(".")[0]

# OK - not accessing first or last element of split
split_with_index_1 = url.split(".")[1]
split_with_index_neg2 = url.split(".")[-2]
