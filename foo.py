import errno
import json
import os
import platform
import shutil
import stat
import subprocess
import sys
import tempfile
from contextlib import contextmanager
from zipfile import ZipFile

import requests
import tomli
from packaging.requirements import Requirement
from packaging.specifiers import SpecifierSet
from virtualenv import cli_run

HERE = os.path.dirname(os.path.abspath(__file__))
ON_WINDOWS = platform.system() == 'Windows'


def handle_remove_readonly(func, path, exc):  # no cov
    # PermissionError: [WinError 5] Access is denied: '...\\.git\\...'
    if func in (os.rmdir, os.remove, os.unlink) and exc[1].errno == errno.EACCES:
        os.chmod(path, stat.S_IRWXU | stat.S_IRWXG | stat.S_IRWXO)
        func(path)
    else:
        raise


class EnvVars(dict):
    def __init__(self, env_vars=None, ignore=None):
        super().__init__(os.environ)
        self.old_env = dict(self)

        if env_vars is not None:
            self.update(env_vars)

        if ignore is not None:
            for env_var in ignore:
                self.pop(env_var, None)

    def __enter__(self):
        os.environ.clear()
        os.environ.update(self)

    def __exit__(self, exc_type, exc_value, traceback):
        os.environ.clear()
        os.environ.update(self.old_env)


def python_version_supported(project_config):
    requires_python = project_config['project'].get('requires-python', '')
    if requires_python:
        python_constraint = SpecifierSet(requires_python)
        if not python_constraint.contains(str('.'.join(map(str, sys.version_info[:2])))):
            return False

    return True


def download_file(url, file_name):
    response = requests.get(url, stream=True)
    with open(file_name, 'wb') as f:
        for chunk in response.iter_content(16384):
            f.write(chunk)


@contextmanager
def temp_dir():
    d = tempfile.mkdtemp()

    try:
        d = os.path.realpath(d)
        yield d
    finally:
        shutil.rmtree(d, ignore_errors=False, onerror=handle_remove_readonly)


def get_venv_exe_dir(venv_dir):
    exe_dir = os.path.join(venv_dir, 'Scripts' if ON_WINDOWS else 'bin')
    if os.path.isdir(exe_dir):
        return exe_dir
    # PyPy
    elif ON_WINDOWS:
        exe_dir = os.path.join(venv_dir, 'bin')
        if os.path.isdir(exe_dir):
            return exe_dir
        else:
            raise OSError(f'Unable to locate executables directory within: {venv_dir}')
    # Debian
    elif os.path.isdir(os.path.join(venv_dir, 'local')):
        exe_dir = os.path.join(venv_dir, 'local', 'bin')
        if os.path.isdir(exe_dir):
            return exe_dir
        else:
            raise OSError(f'Unable to locate executables directory within: {venv_dir}')
    else:
        raise OSError(f'Unable to locate executables directory within: {venv_dir}')


def main():
    original_backend_path = os.path.dirname(os.path.dirname(HERE))
    with temp_dir() as links_dir, temp_dir() as build_dir:
        print('<<<<< Copying backend >>>>>')
        backend_path = os.path.join(build_dir, 'backend')
        shutil.copytree(original_backend_path, backend_path)

        # Increment the minor version
        version_file = os.path.join(backend_path, 'src', 'hatchling', '__about__.py')
        with open(version_file) as f:
            lines = f.readlines()

        for i, line in enumerate(lines):
            if line.startswith('__version__'):
                version = line.strip().split(' = ')[1].strip('\'"')
                version_parts = version.split('.')
                version_parts[1] = str(int(version_parts[1]) + 1)
                lines[i] = line.replace(version, '.'.join(version_parts))
                break
        else:
            raise ValueError('No version found')

        with open(version_file, 'w') as f:
            f.writelines(lines)

        print('<<<<< Building backend >>>>>')
        subprocess.check_call([sys.executable, '-m', 'build', '--wheel', '-o', links_dir, backend_path])
        subprocess.check_call(
            [
                sys.executable,
                '-m',
                'pip',
                'download',
                '-q',
                '--disable-pip-version-check',
                '--no-python-version-warning',
                '-d',
                links_dir,
                os.path.join(links_dir, os.listdir(links_dir)[0]),
            ]
        )

        constraints = []
        constraints_file = os.path.join(build_dir, 'constraints.txt')
        with open(constraints_file, 'w') as f:
            f.write('\n'.join(constraints))

        for project in os.listdir(HERE):
            project_dir = os.path.join(HERE, project)
            if not os.path.isdir(project_dir):
                continue

            print(f'<<<<< Project: {project} >>>>>')
            project_config = {}
            potential_project_file = os.path.join(project_dir, 'pyproject.toml')

            # Not yet ported
            if os.path.isfile(potential_project_file):
                with open(potential_project_file) as f:
                    project_config.update(tomli.loads(f.read()))

                if not python_version_supported(project_config):
                    print('--> Unsupported version of Python, skipping')
                    continue

            with open(os.path.join(project_dir, 'data.json')) as f:
                test_data = json.loads(f.read())

            with temp_dir() as d:
                if 'repo_url' in test_data:
                    print('--> Cloning repository')
                    repo_dir = os.path.join(d, 'repo')
                    subprocess.check_call(['git', 'clone', '-q', '--depth', '1', test_data['repo_url'], repo_dir])
                else:
                    archive_name = f'{project}.zip'
                    archive_path = os.path.join(d, archive_name)

                    print('--> Downloading archive')
                    download_file(test_data['archive_url'], archive_path)
                    with ZipFile(archive_path) as zip_file:
                        zip_file.extractall(d)

                    entries = os.listdir(d)
                    entries.remove(archive_name)
                    repo_dir = os.path.join(d, entries[0])

                project_file = os.path.join(repo_dir, 'pyproject.toml')
                if project_config:
                    shutil.copyfile(potential_project_file, project_file)
                else:
                    if not os.path.isfile(project_file):
                        sys.exit('--> Missing file: pyproject.toml')

                    with open(project_file) as f:
                        project_config.update(tomli.loads(f.read()))

                    for requirement in project_config.get('build-system', {}).get('requires', []):
                        if Requirement(requirement).name == 'hatchling':
                            break
                    else:
                        sys.exit('--> Field `build-system.requires` must specify `hatchling` as a requirement')

                    if not python_version_supported(project_config):
                        print('--> Unsupported version of Python, skipping')
                        continue

                for file_name in ('MANIFEST.in', 'setup.cfg', 'setup.py'):
                    possible_path = os.path.join(repo_dir, file_name)
                    if os.path.isfile(possible_path):
                        os.remove(possible_path)

                venv_dir = os.path.join(d, '.venv')
                print('--> Creating virtual environment')
                cli_run([venv_dir, '--no-download', '--no-periodic-update'])

                env_vars = dict(test_data.get('env_vars', {}))
                env_vars['VIRTUAL_ENV'] = venv_dir
                env_vars['PATH'] = f'{get_venv_exe_dir(venv_dir)}{os.pathsep}{os.environ["PATH"]}'
                env_vars['PIP_CONSTRAINT'] = constraints_file
                with EnvVars(env_vars, ignore=('__PYVENV_LAUNCHER__', 'PYTHONHOME')):
                    print('--> Installing project')
                    subprocess.check_call(
                        [
                            shutil.which('pip'),
                            'install',
                            '-q',
                            '--disable-pip-version-check',
                            '--no-python-version-warning',
                            '--find-links',
                            links_dir,
                            '--no-deps',
                            repo_dir,
                        ]
                    )

                    print('--> Installing dependencies')
                    subprocess.check_call(
                        [
                            shutil.which('pip'),
                            'install',
                            '-q',
                            '--disable-pip-version-check',
                            '--no-python-version-warning',
                            repo_dir,
                        ]
                    )

                    print('--> Testing package')
                    for statement in test_data['statements']:
                        subprocess.check_call([shutil.which('python'), '-c', statement])

                    scripts = project_config['project'].get('scripts', {})
                    if scripts:
                        print('--> Testing scripts')
                        for script in scripts:
                            if not shutil.which(script):
                                sys.exit(f'--> Could not locate script: {script}')

                    print('--> Success!')


if __name__ == '__main__':
    main()
