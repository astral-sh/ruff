import sys
import tarfile
import tempfile


def unsafe_archive_handler(filename):
    tar = tarfile.open(filename)
    tar.extractall(path=tempfile.mkdtemp())
    tar.close()


def managed_members_archive_handler(filename):
    tar = tarfile.open(filename)
    tar.extractall(path=tempfile.mkdtemp(), members=members_filter(tar))
    tar.close()


def list_members_archive_handler(filename):
    tar = tarfile.open(filename)
    tar.extractall(path=tempfile.mkdtemp(), members=[])
    tar.close()


def provided_members_archive_handler(filename):
    tar = tarfile.open(filename)
    tarfile.extractall(path=tempfile.mkdtemp(), members=tar)
    tar.close()


def members_filter(tarfile):
    result = []
    for member in tarfile.getmembers():
        if '../' in member.name:
            print('Member name container directory traversal sequence')
            continue
        elif (member.issym() or member.islnk()) and ('../' in member.linkname):
            print('Symlink to external resource')
            continue
        result.append(member)
    return result


if __name__ == "__main__":
    if len(sys.argv) > 1:
        filename = sys.argv[1]
        unsafe_archive_handler(filename)
        managed_members_archive_handler(filename)
