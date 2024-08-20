import logging
import os
import subprocess
import typing
from pathlib import Path


class Project(typing.NamedTuple):
    name: str
    """The name of the project to benchmark."""

    repository: str
    """The git repository to clone."""

    revision: str

    dependencies: list[str]
    """List of type checking dependencies"""

    include: list[str] = []
    """The directories and files to check. If empty, checks the current directory"""

    pyright_arguments: list[str] | None = None
    """The arguments passed to pyright. Overrides `include` if set."""

    mypy_arguments: list[str] | None = None
    """The arguments passed to mypy. Overrides `include` if set."""

    def clone(self, checkout_dir: Path):
        if os.path.exists(os.path.join(checkout_dir, ".git")):
            return

        logging.debug(f"Cloning {self.repository} to {checkout_dir}")

        try:
            subprocess.run(
                [
                    "git",
                    "init",
                    "--quiet",
                ],
                stderr=subprocess.PIPE,
                env={"GIT_TERMINAL_PROMPT": "0"},
                check=True,
                cwd=checkout_dir,
            )

            subprocess.run(
                ["git", "remote", "add", "origin", str(self.repository), "--no-fetch"],
                env={"GIT_TERMINAL_PROMPT": "0"},
                check=True,
                stderr=subprocess.PIPE,
                cwd=checkout_dir,
            )

            subprocess.run(
                [
                    "git",
                    "fetch",
                    "origin",
                    self.revision,
                    "--quiet",
                    "--depth",
                    "1",
                    "--no-tags",
                ],
                check=True,
                stderr=subprocess.PIPE,
                cwd=checkout_dir,
            )

            subprocess.run(
                ["git", "reset", "--hard", "FETCH_HEAD", "--quiet"],
                check=True,
                stderr=subprocess.PIPE,
                cwd=checkout_dir,
            )

        except subprocess.CalledProcessError as e:
            raise RuntimeError(f"Failed to clone {self.name}: {e.stderr.decode()}")

        logging.info(f"Cloned {self.name} to {checkout_dir}.")


# Selection of projects taken from
# [mypy-primer](https://github.com/hauntsaninja/mypy_primer/blob/0ea6cc614b3e91084059b9a3acc58f94c066a211/mypy_primer/projects.py#L71).
# May require frequent updating, especially the dependencies list
ALL = [
    Project(
        name="black",
        repository="https://github.com/psf/black",
        revision="c20423249e9d8dfb8581eebbfc67a13984ee45e9",
        include=["src"],
        dependencies=[
            "aiohttp",
            "click",
            "pathspec",
            "tomli",
            "platformdirs",
            "packaging",
        ],
    ),
    Project(
        name="jinja",
        repository="https://github.com/pallets/jinja",
        revision="b490da6b23b7ad25dc969976f64dc4ffb0a2c182",
        include=[],
        dependencies=["markupsafe"],
    ),
    Project(
        name="pandas",
        repository="https://github.com/pandas-dev/pandas",
        revision="7945e563d36bcf4694ccc44698829a6221905839",
        include=["pandas"],
        dependencies=[
            "numpy",
            "types-python-dateutil",
            "types-pytz",
            "types-PyMySQL",
            "types-setuptools",
            "pytest",
        ],
    ),
]


DEFAULT: list[str] = ["black", "jinja"]
