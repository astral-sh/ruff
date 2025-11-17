import logging
import subprocess
from pathlib import Path
from typing import Final, Literal, NamedTuple


class Project(NamedTuple):
    name: str
    """The name of the project to benchmark."""

    repository: str
    """The git repository to clone."""

    revision: str

    install_arguments: list[str]
    """Arguments passed to `uv pip install`.

    Dependencies are pinned using a `--exclude-newer` flag when installing them
    into the virtual environment; see the `Venv.install()` method for details.
    """

    python_version: Literal["3.13", "3.14", "3.12", "3.11", "3.10", "3.9", "3.8"]

    include: list[str] = []
    """The directories and files to check. If empty, checks the current directory"""

    exclude: list[str] = []
    """The directories and files to exclude from checks."""

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
ALL: Final = [
    # As of Nov 19th 2025:
    # MyPy: 0
    # Pyright: 40
    # ty: 27
    # Pyrefly: 44
    Project(
        name="black",
        repository="https://github.com/psf/black",
        revision="45b4087976b7880db9dabacc992ee142f2d6c7c7",
        python_version="3.10",
        include=["src"],
        install_arguments=[
            "-r",
            "pyproject.toml",
            # All extras except jupyter because installing the jupyter optional results in a mypy typing error.
            "--extra",
            "colorama",
            "--extra",
            "uvloop",
            "--extra",
            "d",
        ],
    ),
    # As of Nov 19th 2025:
    # MyPy: 941
    # Pyright: 210
    # ty: 260
    # Pyrefly: 652
    Project(
        name="discord.py",
        repository="https://github.com/Rapptz/discord.py.git",
        revision="9be91cb093402f54a44726c7dc4c04ff3b2c5a63",
        python_version="3.8",
        include=["discord"],
        install_arguments=[
            "-r",
            "pyproject.toml",
            "typing_extensions>=4.3,<5",
        ],
    ),
    # Fairly chunky project, requires the pydantic mypy plugin.
    #
    # Pyrefly reports significantely more diagnostics than ty and, unlike ty, has partial pydantic support.
    # Both could be the reason why Pyrefly is slower than ty (it's notable that it's mainly slower because it has a much higher system time)
    #
    # As of Nov 19th 2025:
    # mypy: 12
    # ty: 4'661
    # Pyrefly: 37'610
    # Pyright: 19'186
    Project(
        name="homeassistant",
        repository="https://github.com/home-assistant/core.git",
        revision="10c12623bfc0b3a06ffaa88bf986f61818cfb8be",
        python_version="3.13",
        include=["homeassistant"],
        install_arguments=[
            "-r",
            "requirements_test_all.txt",
            "-r",
            "requirements.txt",
        ],
    ),
    # As of Nov 19th 2025:
    # mypy: 0
    # ty: 19
    # Pyrefly: 18
    # Pyright: 24
    Project(
        name="isort",
        repository="https://github.com/pycqa/isort",
        revision="ed501f10cb5c1b17aad67358017af18cf533c166",
        python_version="3.11",
        include=["isort"],
        install_arguments=["types-colorama", "colorama"],
    ),
    # As of Nov 19th 2025:
    # mypy: 6 (all unused-ignore)
    # ty: 22
    # Pyrefly: 65
    # Pyright: 44
    Project(
        name="jinja",
        repository="https://github.com/pallets/jinja",
        revision="5ef70112a1ff19c05324ff889dd30405b1002044",
        python_version="3.10",
        include=["src"],
        install_arguments=["-r", "pyproject.toml"],
    ),
    # As of Nov 19th 2025:
    # mypy: 0
    # ty: 784
    # Pyrefly: 1'602
    # Pyright: 2'180
    Project(
        name="pandas",
        repository="https://github.com/pandas-dev/pandas",
        revision="4d8348341bc4de2f0f90782ecef1b092b9418a19",
        include=["pandas", "typings"],
        exclude=["pandas/tests"],
        python_version="3.11",
        install_arguments=[
            "-r",
            "requirements-dev.txt",
        ],
    ),
    # As of Nov 19th 2025:
    # mypy: 0
    # ty: 4
    # Pyrefly: 0
    # Pyright: 0
    Project(
        name="pandas-stubs",
        repository="https://github.com/pandas-dev/pandas-stubs",
        revision="ad8cae5bc1f0bc87ce22b4d445e0700976c9dfb4",
        include=["pandas-stubs"],
        python_version="3.10",
        # Uses poetry :(
        install_arguments=[
            "types-pytz >=2022.1.1",
            "types-python-dateutil>=2.8.19",
            "numpy >=1.23.5",
            "pyarrow >=10.0.1",
            "matplotlib >=3.10.1",
            "xarray>=22.6.0",
            "SQLAlchemy>=2.0.39",
            "odfpy >=1.4.1",
            "pyxlsb >=1.0.10",
            "jinja2 >=3.1",
            "scipy >=1.9.1",
            "scipy-stubs >=1.15.3.0",
        ],
    ),
    # Requires the pydantic mypy plugin
    # As of Nov 19th 2025:
    # mypy: 1
    # ty: 119
    # Pyrefly: 159
    # Pyright: 174
    Project(
        name="prefect",
        repository="https://github.com/PrefectHQ/prefect.git",
        revision="a3db33d4f9ee7a665430ae6017c649d057139bd3",
        # See https://github.com/PrefectHQ/prefect/blob/a3db33d4f9ee7a665430ae6017c649d057139bd3/.pre-commit-config.yaml#L33-L39
        include=[
            "src/prefect/server/models",
            "src/prefect/concurrency",
            "src/prefect/events",
            "src/prefect/input",
        ],
        python_version="3.10",
        install_arguments=[
            "-r",
            "pyproject.toml",
            "--group",
            "dev",
        ],
    ),
    # Requires sympy-mypy plugin.
    # As of Nov 19th 2025:
    # mypy: 16'623
    # ty: 23'231
    # Pyrefly: 15,877
    # Pyright: 19'684
    Project(
        name="pytorch",
        repository="https://github.com/pytorch/pytorch.git",
        revision="be33b7faf685560bb618561b44b751713a660337",
        include=["torch", "caffe2"],
        # see https://github.com/pytorch/pytorch/blob/c56655268b4ae575ee4c89c312fd93ca2f5b3ba9/pyrefly.toml#L23
        exclude=[
            "torch/_inductor/codegen/triton.py",
            "tools/linter/adapters/test_device_bias_linter.py",
            "tools/code_analyzer/gen_operators_yaml.py",
            "torch/_inductor/runtime/triton_heuristics.py",
            "torch/_inductor/runtime/triton_helpers.py",
            "torch/_inductor/runtime/halide_helpers.py",
            "torch/utils/tensorboard/summary.py",
            "torch/distributed/flight_recorder/components/types.py",
            "torch/linalg/__init__.py",
            "torch/package/importer.py",
            "torch/package/_package_pickler.py",
            "torch/jit/annotations.py",
            "torch/utils/data/datapipes/_typing.py",
            "torch/nn/functional.py",
            "torch/_export/utils.py",
            "torch/fx/experimental/unification/multipledispatch/__init__.py",
            "torch/nn/modules/__init__.py",
            "torch/nn/modules/rnn.py",
            "torch/_inductor/codecache.py",
            "torch/distributed/elastic/metrics/__init__.py",
            "torch/_inductor/fx_passes/bucketing.py",
            "torch/onnx/_internal/exporter/_torchlib/ops/nn.py",
            "torch/include/**",
            "torch/csrc/**",
            "torch/distributed/elastic/agent/server/api.py",
            "torch/testing/_internal/**",
            "torch/distributed/fsdp/fully_sharded_data_parallel.py",
            "torch/ao/quantization/pt2e/_affine_quantization.py",
            "torch/nn/modules/pooling.py",
            "torch/nn/parallel/_functions.py",
            "torch/_appdirs.py",
            "torch/multiprocessing/pool.py",
            "torch/overrides.py",
            "*/__pycache__/**",
            "*/.*",
        ],
        # See https://github.com/pytorch/pytorch/blob/be33b7faf685560bb618561b44b751713a660337/.lintrunner.toml#L141
        install_arguments=[
            'numpy==1.26.4 ; python_version >= "3.10" and python_version <= "3.11"',
            'numpy==2.1.0 ; python_version >= "3.12" and python_version <= "3.13"',
            'numpy==2.3.4 ; python_version >= "3.14"',
            "expecttest==0.3.0",
            "pyrefly==0.36.2",
            "sympy==1.13.3",
            "types-requests==2.27.25",
            "types-pyyaml==6.0.2",
            "types-tabulate==0.8.8",
            "types-protobuf==5.29.1.20250403",
            "types-setuptools==79.0.0.20250422",
            "types-jinja2==2.11.9",
            "types-colorama==0.4.6",
            "filelock==3.18.0",
            "junitparser==2.1.1",
            "rich==14.1.0",
            "optree==0.17.0",
            "types-openpyxl==3.1.5.20250919",
            "types-python-dateutil==2.9.0.20251008",
            "mypy==1.16.0",  # pytorch pinns mypy,
        ],
        python_version="3.11",
    ),
]
