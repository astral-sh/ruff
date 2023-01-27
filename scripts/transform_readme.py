"""Transform the README.md to support a specific deployment target.

By default, we assume that our README.md will be rendered on GitHub. However, different
targets have different strategies for rendering light- and dark-mode images. This script
adjusts the images in the README.md to support the given target.
"""
import argparse
from pathlib import Path

# https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#specifying-the-theme-an-image-is-shown-to
GITHUB = """
<p align="center">
  <picture align="center">
    <source media="(prefers-color-scheme: dark)" srcset="https://user-images.githubusercontent.com/1309177/212613422-7faaf278-706b-4294-ad92-236ffcab3430.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://user-images.githubusercontent.com/1309177/212613257-5f4bca12-6d6b-4c79-9bac-51a4c6d08928.svg">
    <img alt="Shows a bar chart with benchmark results." src="https://user-images.githubusercontent.com/1309177/212613257-5f4bca12-6d6b-4c79-9bac-51a4c6d08928.svg">
  </picture>
</p>
"""

# https://github.com/pypi/warehouse/issues/11251
PYPI = """
<p align="center">
  <picture align="center">
    <img alt="Shows a bar chart with benchmark results." src="https://user-images.githubusercontent.com/1309177/212613257-5f4bca12-6d6b-4c79-9bac-51a4c6d08928.svg">
  </picture>
</p>
"""

# https://squidfunk.github.io/mkdocs-material/reference/images/#light-and-dark-mode
MK_DOCS = """
<p align="center">
  <picture align="center">
    <img alt="Shows a bar chart with benchmark results." src="https://user-images.githubusercontent.com/1309177/212613257-5f4bca12-6d6b-4c79-9bac-51a4c6d08928.svg#only-light">
  </picture>
</p>

<p align="center">
  <picture align="center">
    <img alt="Shows a bar chart with benchmark results." src="https://user-images.githubusercontent.com/1309177/212613422-7faaf278-706b-4294-ad92-236ffcab3430.svg#only-dark">
  </picture>
</p>
"""


def main(target: str) -> None:
    """Modify the README.md to support the given target."""
    with Path("README.md").open(encoding="utf8") as fp:
        content = fp.read()
        if GITHUB not in content:
            msg = "README.md is not in the expected format."
            raise ValueError(msg)

    if target == "pypi":
        with Path("README.md").open("w", encoding="utf8") as fp:
            fp.write(content.replace(GITHUB, PYPI))
    elif target == "mkdocs":
        with Path("README.md").open("w", encoding="utf8") as fp:
            fp.write(content.replace(GITHUB, MK_DOCS))
    else:
        msg = f"Unknown target: {target}"
        raise ValueError(msg)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Modify the README.md to support a specific deployment target.",
    )
    parser.add_argument(
        "--target",
        type=str,
        required=True,
        choices=("pypi", "mkdocs"),
    )
    args = parser.parse_args()

    main(target=args.target)
