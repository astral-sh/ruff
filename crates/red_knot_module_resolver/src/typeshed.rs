pub(crate) mod versions;

#[cfg(test)]
mod tests {
    use std::io::{self, Read};

    #[test]
    fn typeshed_zip_created_at_build_time() -> anyhow::Result<()> {
        // The file path here is hardcoded in this crate's `build.rs` script.
        // Luckily this crate will fail to build if this file isn't available at build time.
        const TYPESHED_ZIP_BYTES: &[u8] =
            include_bytes!(concat!(env!("OUT_DIR"), "/zipped_typeshed.zip"));

        let mut typeshed_zip_archive = zip::ZipArchive::new(io::Cursor::new(TYPESHED_ZIP_BYTES))?;

        let mut functools_module_stub = typeshed_zip_archive
            .by_name("stdlib/functools.pyi")
            .unwrap();
        assert!(functools_module_stub.is_file());

        let mut functools_module_stub_source = String::new();
        functools_module_stub.read_to_string(&mut functools_module_stub_source)?;

        assert!(functools_module_stub_source.contains("def update_wrapper("));
        Ok(())
    }
}
