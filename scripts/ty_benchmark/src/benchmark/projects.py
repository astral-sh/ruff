import logging
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
    """List of type checking dependencies.

    Dependencies are pinned using a `--exclude-newer` flag when installing them
    into the virtual environment; see the `Venv.install()` method for details.
    """

    include: list[str] = []
    """The directories and files to check. If empty, checks the current directory"""

    pyright_arguments: list[str] | None = None
    """The arguments passed to pyright. Overrides `include` if set."""

    mypy_arguments: list[str] | None = None
    """The arguments passed to mypy. Overrides `include` if set."""

    def clone(self, checkout_dir: Path) -> None:
        # Skip cloning if the project has already been cloned (the script doesn't yet support updating)
        if (checkout_dir / ".git").exists():
            return

        logging.debug(f"Cloning {self.repository} to {checkout_dir}")

        try:
            # git doesn't support cloning a specific revision.
            # This is the closest that I found to a "shallow clone with a specific revision"
            subprocess.run(
                [
                    "git",
                    "init",
                    "--quiet",
                ],
                env={"GIT_TERMINAL_PROMPT": "0"},
                cwd=checkout_dir,
                check=True,
                capture_output=True,
                text=True,
            )

            subprocess.run(
                ["git", "remote", "add", "origin", str(self.repository), "--no-fetch"],
                env={"GIT_TERMINAL_PROMPT": "0"},
                cwd=checkout_dir,
                check=True,
                capture_output=True,
                text=True,
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
                cwd=checkout_dir,
                capture_output=True,
                text=True,
            )

            subprocess.run(
                ["git", "reset", "--hard", "FETCH_HEAD", "--quiet"],
                check=True,
                cwd=checkout_dir,
                capture_output=True,
                text=True,
            )

        except subprocess.CalledProcessError as e:
            raise RuntimeError(f"Failed to clone {self.name}: {e.stderr}")

        logging.info(f"Cloned {self.name} to {checkout_dir}.")


# Selection of projects taken from
# [mypy-primer](https://github.com/hauntsaninja/mypy_primer/blob/0ea6cc614b3e91084059b9a3acc58f94c066a211/mypy_primer/projects.py#L71).
# May require frequent updating, especially the dependencies list
ALL = [
    Project(
        name="black",
        repository="https://github.com/psf/black",
        revision="ac28187bf4a4ac159651c73d3a50fe6d0f653eac",
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
    Project(
        name="isort",
        repository="https://github.com/pycqa/isort",
        revision="7de182933fd50e04a7c47cc8be75a6547754b19c",
        mypy_arguments=["--ignore-missing-imports", "isort"],
        include=["isort"],
        dependencies=["types-setuptools"],
    ),
]


DEFAULT: list[str] = ["black", "jinja", "isort"]
