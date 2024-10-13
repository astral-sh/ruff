import os
import sys
import sysconfig


def find_ruff_bin() -> str:
    """Return the ruff binary path."""

    ruff_exe = "ruff" + sysconfig.get_config_var("EXE")

    scripts_path = os.path.join(sysconfig.get_path("scripts"), ruff_exe)
    if os.path.isfile(scripts_path):
        return scripts_path

    if sys.version_info >= (3, 10):
        user_scheme = sysconfig.get_preferred_scheme("user")
    elif os.name == "nt":
        user_scheme = "nt_user"
    elif sys.platform == "darwin" and sys._framework:
        user_scheme = "osx_framework_user"
    else:
        user_scheme = "posix_user"

    user_path = os.path.join(
        sysconfig.get_path("scripts", scheme=user_scheme), ruff_exe
    )
    if os.path.isfile(user_path):
        return user_path

    # Search in `bin` adjacent to package root (as created by `pip install --target`).
    pkg_root = os.path.dirname(os.path.dirname(__file__))
    target_path = os.path.join(pkg_root, "bin", ruff_exe)
    if os.path.isfile(target_path):
        return target_path

    # Search for pip-specific build environments.
    #
    # See: https://github.com/pypa/pip/blob/102d8187a1f5a4cd5de7a549fd8a9af34e89a54f/src/pip/_internal/build_env.py#L87
    paths = os.environ.get("PATH", "").split(os.pathsep)
    if len(paths) >= 2:
        first, second = os.path.split(paths[0]), os.path.split(paths[1])
        # Search for both an `overlay` and `normal` folder within a `pip-build-env-{random}` folder. (The final segment
        # of the path is the `bin` directory.)
        if (
            len(first) >= 3
            and len(second) >= 3
            and first[-3].startswith("pip-build-env-")
            and first[-2] == "overlay"
            and second[-3].startswith("pip-build-env-")
            and second[-2] == "normal"
        ):
            # The overlay must contain the ruff binary.
            candidate = os.path.join(first, ruff_exe)
            if os.path.isfile(candidate):
                return candidate

    raise FileNotFoundError(scripts_path)


if __name__ == "__main__":
    ruff = os.fsdecode(find_ruff_bin())
    if sys.platform == "win32":
        import subprocess

        completed_process = subprocess.run([ruff, *sys.argv[1:]])
        sys.exit(completed_process.returncode)
    else:
        os.execvp(ruff, [ruff, *sys.argv[1:]])
