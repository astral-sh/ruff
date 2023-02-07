__all__ = "CONST"  # [invalid-all-format]

__all__ = ["Hello"] + {"world"}  # [invalid-all-format]

__all__ += {"world"}  # [invalid-all-format]

__all__ = {"world"} + ["Hello"]  # [invalid-all-format]

__all__ = {"world"} + list(["Hello"])  # [invalid-all-format]

__all__ = list(["Hello"]) + {"world"}  # [invalid-all-format]

__all__ = (x for x in ["Hello", "world"])  # [invalid-all-format]

__all__ = {x for x in ["Hello", "world"]}  # [invalid-all-format]

__all__ = ["Hello"]

__all__ = ("Hello",)

__all__ = ["Hello"] + ("world",)

__all__ = [x for x in ["Hello", "world"]]

__all__ = list(["Hello", "world"])

__all__ = list({"Hello", "world"})

__all__ = list(["Hello"]) + list(["world"])

__all__ = tuple(["Hello"]) + ("world",)
