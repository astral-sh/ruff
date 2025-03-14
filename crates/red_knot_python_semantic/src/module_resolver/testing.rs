use ruff_db::system::{
    DbWithTestSystem as _, DbWithWritableSystem as _, SystemPath, SystemPathBuf,
};
use ruff_db::vendored::VendoredPathBuf;
use ruff_python_ast::PythonVersion;

use crate::db::tests::TestDb;
use crate::program::{Program, SearchPathSettings};
use crate::{ProgramSettings, PythonPath, PythonPlatform};

/// A test case for the module resolver.
///
/// You generally shouldn't construct instances of this struct directly;
/// instead, use the [`TestCaseBuilder`].
pub(crate) struct TestCase<T> {
    pub(crate) db: TestDb,
    pub(crate) src: SystemPathBuf,
    pub(crate) stdlib: T,
    // Most test cases only ever need a single `site-packages` directory,
    // so this is a single directory instead of a `Vec` of directories,
    // like it is in `ruff_db::Program`.
    pub(crate) site_packages: SystemPathBuf,
    pub(crate) python_version: PythonVersion,
}

/// A `(file_name, file_contents)` tuple
pub(crate) type FileSpec = (&'static str, &'static str);

/// Specification for a typeshed mock to be created as part of a test
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MockedTypeshed {
    /// The stdlib files to be created in the typeshed mock
    pub(crate) stdlib_files: &'static [FileSpec],

    /// The contents of the `stdlib/VERSIONS` file
    /// to be created in the typeshed mock
    pub(crate) versions: &'static str,
}

#[derive(Debug)]
pub(crate) struct VendoredTypeshed;

#[derive(Debug)]
pub(crate) struct UnspecifiedTypeshed;

/// A builder for a module-resolver test case.
///
/// The builder takes care of creating a [`TestDb`]
/// instance, applying the module resolver settings,
/// and creating mock directories for the stdlib, `site-packages`,
/// first-party code, etc.
///
/// For simple tests that do not involve typeshed,
/// test cases can be created as follows:
///
/// ```rs
/// let test_case = TestCaseBuilder::new()
///     .with_src_files(...)
///     .build();
///
/// let test_case2 = TestCaseBuilder::new()
///     .with_site_packages_files(...)
///     .build();
/// ```
///
/// Any tests can specify the target Python version that should be used
/// in the module resolver settings:
///
/// ```rs
/// let test_case = TestCaseBuilder::new()
///     .with_src_files(...)
///     .with_python_version(...)
///     .build();
/// ```
///
/// For tests checking that standard-library module resolution is working
/// correctly, you should usually create a [`MockedTypeshed`] instance
/// and pass it to the [`TestCaseBuilder::with_mocked_typeshed`] method.
/// If you need to check something that involves the vendored typeshed stubs
/// we include as part of the binary, you can instead use the
/// [`TestCaseBuilder::with_vendored_typeshed`] method.
/// For either of these, you should almost always try to be explicit
/// about the Python version you want to be specified in the module-resolver
/// settings for the test:
///
/// ```rs
/// const TYPESHED = MockedTypeshed { ... };
///
/// let test_case = resolver_test_case()
///     .with_mocked_typeshed(TYPESHED)
///     .with_python_version(...)
///     .build();
///
/// let test_case2 = resolver_test_case()
///     .with_vendored_typeshed()
///     .with_python_version(...)
///     .build();
/// ```
///
/// If you have not called one of those options, the `stdlib` field
/// on the [`TestCase`] instance created from `.build()` will be set
/// to `()`.
pub(crate) struct TestCaseBuilder<T> {
    typeshed_option: T,
    python_version: PythonVersion,
    python_platform: PythonPlatform,
    first_party_files: Vec<FileSpec>,
    site_packages_files: Vec<FileSpec>,
}

impl<T> TestCaseBuilder<T> {
    /// Specify files to be created in the `src` mock directory
    pub(crate) fn with_src_files(mut self, files: &[FileSpec]) -> Self {
        self.first_party_files.extend(files.iter().copied());
        self
    }

    /// Specify files to be created in the `site-packages` mock directory
    pub(crate) fn with_site_packages_files(mut self, files: &[FileSpec]) -> Self {
        self.site_packages_files.extend(files.iter().copied());
        self
    }

    /// Specify the Python version the module resolver should assume
    pub(crate) fn with_python_version(mut self, python_version: PythonVersion) -> Self {
        self.python_version = python_version;
        self
    }

    fn write_mock_directory(
        db: &mut TestDb,
        location: impl AsRef<SystemPath>,
        files: impl IntoIterator<Item = FileSpec>,
    ) -> SystemPathBuf {
        let root = location.as_ref().to_path_buf();
        // Make sure to create the directory even if the list of files is empty:
        db.memory_file_system().create_directory_all(&root).unwrap();
        db.write_files(
            files
                .into_iter()
                .map(|(relative_path, contents)| (root.join(relative_path), contents)),
        )
        .unwrap();
        root
    }
}

impl TestCaseBuilder<UnspecifiedTypeshed> {
    pub(crate) fn new() -> TestCaseBuilder<UnspecifiedTypeshed> {
        Self {
            typeshed_option: UnspecifiedTypeshed,
            python_version: PythonVersion::default(),
            python_platform: PythonPlatform::default(),
            first_party_files: vec![],
            site_packages_files: vec![],
        }
    }

    /// Use the vendored stdlib stubs included in the Ruff binary for this test case
    pub(crate) fn with_vendored_typeshed(self) -> TestCaseBuilder<VendoredTypeshed> {
        let TestCaseBuilder {
            typeshed_option: _,
            python_version,
            python_platform,
            first_party_files,
            site_packages_files,
        } = self;
        TestCaseBuilder {
            typeshed_option: VendoredTypeshed,
            python_version,
            python_platform,
            first_party_files,
            site_packages_files,
        }
    }

    /// Use a mock typeshed directory for this test case
    pub(crate) fn with_mocked_typeshed(
        self,
        typeshed: MockedTypeshed,
    ) -> TestCaseBuilder<MockedTypeshed> {
        let TestCaseBuilder {
            typeshed_option: _,
            python_version,
            python_platform,
            first_party_files,
            site_packages_files,
        } = self;

        TestCaseBuilder {
            typeshed_option: typeshed,
            python_version,
            python_platform,
            first_party_files,
            site_packages_files,
        }
    }

    pub(crate) fn build(self) -> TestCase<()> {
        let TestCase {
            db,
            src,
            stdlib: _,
            site_packages,
            python_version,
        } = self.with_mocked_typeshed(MockedTypeshed::default()).build();

        TestCase {
            db,
            src,
            stdlib: (),
            site_packages,
            python_version,
        }
    }
}

impl TestCaseBuilder<MockedTypeshed> {
    pub(crate) fn build(self) -> TestCase<SystemPathBuf> {
        let TestCaseBuilder {
            typeshed_option,
            python_version,
            python_platform,
            first_party_files,
            site_packages_files,
        } = self;

        let mut db = TestDb::new();

        let site_packages =
            Self::write_mock_directory(&mut db, "/site-packages", site_packages_files);
        let src = Self::write_mock_directory(&mut db, "/src", first_party_files);
        let typeshed = Self::build_typeshed_mock(&mut db, &typeshed_option);

        Program::from_settings(
            &db,
            ProgramSettings {
                python_version,
                python_platform,
                search_paths: SearchPathSettings {
                    extra_paths: vec![],
                    src_roots: vec![src.clone()],
                    custom_typeshed: Some(typeshed.clone()),
                    python_path: PythonPath::KnownSitePackages(vec![site_packages.clone()]),
                },
            },
        )
        .expect("Valid program settings");

        TestCase {
            db,
            src,
            stdlib: typeshed.join("stdlib"),
            site_packages,
            python_version,
        }
    }

    fn build_typeshed_mock(db: &mut TestDb, typeshed_to_build: &MockedTypeshed) -> SystemPathBuf {
        let typeshed = SystemPathBuf::from("/typeshed");
        let MockedTypeshed {
            stdlib_files,
            versions,
        } = typeshed_to_build;
        Self::write_mock_directory(
            db,
            typeshed.join("stdlib"),
            stdlib_files
                .iter()
                .copied()
                .chain(std::iter::once(("VERSIONS", *versions))),
        );
        typeshed
    }
}

impl TestCaseBuilder<VendoredTypeshed> {
    pub(crate) fn build(self) -> TestCase<VendoredPathBuf> {
        let TestCaseBuilder {
            typeshed_option: VendoredTypeshed,
            python_version,
            python_platform,
            first_party_files,
            site_packages_files,
        } = self;

        let mut db = TestDb::new();

        let site_packages =
            Self::write_mock_directory(&mut db, "/site-packages", site_packages_files);
        let src = Self::write_mock_directory(&mut db, "/src", first_party_files);

        Program::from_settings(
            &db,
            ProgramSettings {
                python_version,
                python_platform,
                search_paths: SearchPathSettings {
                    python_path: PythonPath::KnownSitePackages(vec![site_packages.clone()]),
                    ..SearchPathSettings::new(vec![src.clone()])
                },
            },
        )
        .expect("Valid search path settings");

        TestCase {
            db,
            src,
            stdlib: VendoredPathBuf::from("stdlib"),
            site_packages,
            python_version,
        }
    }
}
