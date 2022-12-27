from setuptools import setup
from setuptools_rust import Binding, RustExtension, RustBin, Strip

setup(
    name="ruff",
    rust_extensions=[
        RustExtension("ruff._ruff", binding=Binding.PyO3, strip=Strip.Debug, py_limited_api=True),
        RustBin("ruff", strip=Strip.Debug),
    ],
    packages=["ruff"],
    # rust extensions are not zip safe, just like C-extensions.
    zip_safe=False,
)
