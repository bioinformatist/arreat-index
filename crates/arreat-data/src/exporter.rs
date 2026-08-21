use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

#[cfg(target_os = "linux")]
use std::ffi::CString;

use crate::error::{self, Error, Result};

pub const SOURCE_WHITELIST: &[&str] = &[
    "data/global/excel/armor.txt",
    "data/global/excel/automagic.txt",
    "data/global/excel/itemstatcost.txt",
    "data/global/excel/itemtypes.txt",
    "data/global/excel/magicprefix.txt",
    "data/global/excel/magicsuffix.txt",
    "data/global/excel/misc.txt",
    "data/global/excel/properties.txt",
    "data/global/excel/runes.txt",
    "data/global/excel/setitems.txt",
    "data/global/excel/skills.txt",
    "data/global/excel/uniqueitems.txt",
    "data/global/excel/weapons.txt",
    "data/local/lng/strings/item-nameaffixes.json",
    "data/local/lng/strings/item-names.json",
];

const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
#[cfg(target_os = "linux")]
const READ_CHUNK_BYTES: usize = 64 * 1024;

/// A read-only adapter at the exporter seam.
///
/// Implementations must stream the named file into `destination`, return an
/// error without retaining the destination on failure, and never write to the
/// source storage.
pub trait ArchiveReader {
    fn copy_named(&mut self, name: &str, destination: &mut dyn Write) -> Result<()>;
}

pub fn export_archive(game_root: &Path, output: &Path) -> Result<()> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (game_root, output);
        return Err(Error::UnsupportedPlatform);
    }

    #[cfg(target_os = "linux")]
    {
        let root = fs::canonicalize(game_root).map_err(|source| error::io(game_root, source))?;
        ensure_output_outside_root(&root, output)?;
        let build_path = root.join(".build.info");
        let canonical_build =
            fs::canonicalize(&build_path).map_err(|source| error::io(&build_path, source))?;
        if !canonical_build.starts_with(&root) {
            return Err(Error::UnsafePath(build_path.display().to_string()));
        }
        let build_info =
            fs::read(&canonical_build).map_err(|source| error::io(&canonical_build, source))?;
        if build_info.len() as u64 > MAX_FILE_BYTES {
            return Err(Error::Message(".build.info exceeds 64 MiB".to_owned()));
        }
        let mut reader = casc::CascReader::open(&root)?;
        export_with_reader(&mut reader, &build_info, output)
    }
}

pub fn export_with_reader(
    reader: &mut dyn ArchiveReader,
    build_info: &[u8],
    output: &Path,
) -> Result<()> {
    if build_info.len() as u64 > MAX_FILE_BYTES {
        return Err(Error::Message(".build.info exceeds 64 MiB".to_owned()));
    }
    validate_archive_name(".build.info")?;
    for name in SOURCE_WHITELIST {
        validate_archive_name(name)?;
    }

    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| error::io(parent, source))?;
    let output_name = output
        .file_name()
        .ok_or_else(|| Error::Message("archive output must name a file".to_owned()))?
        .to_string_lossy();
    let temporary_archive = parent.join(format!(".{output_name}.tmp-{}", std::process::id()));
    let staging = parent.join(format!(".{output_name}.stage-{}", std::process::id()));
    fs::create_dir(&staging).map_err(|source| error::io(&staging, source))?;
    let cleanup = Cleanup {
        archive: temporary_archive.clone(),
        staging: staging.clone(),
        armed: true,
    };

    let result = (|| {
        let archive_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_archive)
            .map_err(|source| error::io(&temporary_archive, source))?;
        let mut archive = tar::Builder::new(archive_file);
        archive.mode(tar::HeaderMode::Deterministic);
        append_bytes(&mut archive, ".build.info", build_info)?;

        for (index, name) in SOURCE_WHITELIST.iter().enumerate() {
            let staged_path = staging.join(format!("{index:02}.input"));
            let staged_file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staged_path)
                .map_err(|source| error::io(&staged_path, source))?;
            let mut limited = LimitedWriter::new(staged_file, MAX_FILE_BYTES);
            reader.copy_named(name, &mut limited)?;
            let mut staged_file = limited.finish()?;
            staged_file
                .flush()
                .map_err(|source| error::io(&staged_path, source))?;
            let length = staged_file
                .metadata()
                .map_err(|source| error::io(&staged_path, source))?
                .len();
            drop(staged_file);
            let mut staged_file =
                File::open(&staged_path).map_err(|source| error::io(&staged_path, source))?;
            append_reader(&mut archive, name, length, &mut staged_file)?;
            fs::remove_file(&staged_path).map_err(|source| error::io(&staged_path, source))?;
        }
        archive
            .finish()
            .map_err(|source| error::io(&temporary_archive, source))?;
        let archive_file = archive
            .into_inner()
            .map_err(|source| error::io(&temporary_archive, source))?;
        archive_file
            .sync_all()
            .map_err(|source| error::io(&temporary_archive, source))?;
        fs::rename(&temporary_archive, output).map_err(|source| error::io(output, source))?;
        Ok(())
    })();

    if result.is_ok() {
        fs::remove_dir(&staging).map_err(|source| error::io(&staging, source))?;
        cleanup.disarm();
    }
    result
}

fn append_bytes(archive: &mut tar::Builder<File>, name: &str, bytes: &[u8]) -> Result<()> {
    append_reader(archive, name, bytes.len() as u64, &mut &*bytes)
}

fn append_reader(
    archive: &mut tar::Builder<File>,
    name: &str,
    length: u64,
    reader: &mut dyn Read,
) -> Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(length);
    header.set_mode(0o644);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_cksum();
    archive
        .append_data(&mut header, name, reader)
        .map_err(|source| Error::Message(format!("could not append {name} to archive: {source}")))
}

fn validate_archive_name(name: &str) -> Result<()> {
    let path = Path::new(name);
    if name.contains('\\')
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(Error::UnsafePath(name.to_owned()));
    }
    Ok(())
}

fn casc_lookup_name(name: &str) -> Result<String> {
    validate_archive_name(name)?;
    Ok(format!("data:{name}"))
}

#[cfg(target_os = "linux")]
fn ensure_output_outside_root(root: &Path, output: &Path) -> Result<()> {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let canonical_parent = fs::canonicalize(parent).map_err(|source| error::io(parent, source))?;
    if canonical_parent.starts_with(root) {
        return Err(Error::Message(format!(
            "archive output must not be below game root {}",
            root.display()
        )));
    }
    Ok(())
}

struct LimitedWriter<W> {
    inner: W,
    remaining: u64,
    exceeded: bool,
}

impl<W> LimitedWriter<W> {
    fn new(inner: W, limit: u64) -> Self {
        Self {
            inner,
            remaining: limit,
            exceeded: false,
        }
    }

    fn finish(self) -> Result<W> {
        if self.exceeded {
            Err(Error::Message("archive member exceeds 64 MiB".to_owned()))
        } else {
            Ok(self.inner)
        }
    }
}

impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            self.exceeded = true;
            return Err(std::io::Error::other("archive member exceeds 64 MiB"));
        }
        let written = self.inner.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

struct Cleanup {
    archive: PathBuf,
    staging: PathBuf,
    armed: bool,
}

impl Cleanup {
    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.archive);
            let _ = fs::remove_dir_all(&self.staging);
        }
    }
}

#[cfg(target_os = "linux")]
mod casc {
    use std::{ffi::c_void, os::raw::c_char};

    use super::*;

    type Handle = *mut c_void;

    unsafe extern "C" {
        fn CascOpenStorage(path: *const c_char, locale_mask: u32, handle: *mut Handle) -> bool;
        fn CascOpenFile(
            storage: Handle,
            name: *const c_void,
            locale_flags: u32,
            open_flags: u32,
            handle: *mut Handle,
        ) -> bool;
        fn CascGetFileSize(file: Handle, high: *mut u32) -> u32;
        fn CascReadFile(
            file: Handle,
            buffer: *mut c_void,
            bytes_to_read: u32,
            bytes_read: *mut u32,
        ) -> bool;
        fn CascCloseFile(file: Handle) -> bool;
        fn CascCloseStorage(storage: Handle) -> bool;
        fn GetCascError() -> u32;
    }

    pub(super) struct CascReader {
        storage: StorageHandle,
    }

    struct StorageHandle(Handle);
    struct FileHandle(Handle);

    impl CascReader {
        pub(super) fn open(root: &Path) -> Result<Self> {
            let root_text = root
                .to_str()
                .ok_or_else(|| Error::Message("game root is not valid UTF-8".to_owned()))?;
            let path = CString::new(root_text)
                .map_err(|_| Error::Message("game root contains an embedded NUL".to_owned()))?;
            let mut handle = std::ptr::null_mut();
            // SAFETY: `path` is NUL terminated and remains alive for the call;
            // `handle` is a valid out pointer. On success CascLib transfers one
            // owned storage handle, which `StorageHandle::drop` closes once.
            let opened = unsafe { CascOpenStorage(path.as_ptr(), u32::MAX, &mut handle) };
            if !opened {
                return Err(casc_error("open storage"));
            }
            Ok(Self {
                storage: StorageHandle(handle),
            })
        }
    }

    impl ArchiveReader for CascReader {
        fn copy_named(&mut self, name: &str, destination: &mut dyn Write) -> Result<()> {
            let lookup_name = casc_lookup_name(name)?;
            let lookup_name_c = CString::new(lookup_name.as_str())
                .map_err(|_| Error::Message("archive name contains an embedded NUL".to_owned()))?;
            let mut handle = std::ptr::null_mut();
            // SAFETY: the storage handle is live for this call,
            // `lookup_name_c` is NUL terminated, and `handle` is a valid out
            // pointer. `FileHandle` takes ownership only when CascOpenFile
            // succeeds.
            let opened = unsafe {
                CascOpenFile(
                    self.storage.0,
                    lookup_name_c.as_ptr().cast(),
                    u32::MAX,
                    0,
                    &mut handle,
                )
            };
            if !opened {
                return Err(casc_member_error("open named file", &lookup_name));
            }
            let file = FileHandle(handle);
            let mut high = 0_u32;
            // SAFETY: `file` owns a live file handle and `high` is a valid out
            // pointer. The handle remains alive through the read loop.
            let low = unsafe { CascGetFileSize(file.0, &mut high) };
            let size = (u64::from(high) << 32) | u64::from(low);
            if size > MAX_FILE_BYTES {
                return Err(Error::Message(format!(
                    "CASC member is {size} bytes; limit is {MAX_FILE_BYTES}"
                )));
            }
            let mut remaining = size;
            let mut buffer = [0_u8; READ_CHUNK_BYTES];
            while remaining > 0 {
                let request = usize::try_from(remaining.min(READ_CHUNK_BYTES as u64))
                    .expect("bounded chunk fits usize");
                let mut read = 0_u32;
                // SAFETY: the file handle is live, the buffer holds `request`
                // writable bytes, and `read` is a valid out pointer.
                let succeeded = unsafe {
                    CascReadFile(
                        file.0,
                        buffer.as_mut_ptr().cast(),
                        request as u32,
                        &mut read,
                    )
                };
                if !succeeded {
                    return Err(casc_member_error("read file", &lookup_name));
                }
                if read == 0 || read as usize > request {
                    return Err(Error::Message(
                        "CascLib returned an invalid short read".to_owned(),
                    ));
                }
                destination
                    .write_all(&buffer[..read as usize])
                    .map_err(|source| error::io("archive staging file", source))?;
                remaining -= u64::from(read);
            }
            Ok(())
        }
    }

    impl Drop for FileHandle {
        fn drop(&mut self) {
            // SAFETY: this wrapper uniquely owns the live handle and Drop runs
            // once. CascLib permits close on every successful open path.
            unsafe {
                CascCloseFile(self.0);
            }
        }
    }

    impl Drop for StorageHandle {
        fn drop(&mut self) {
            // SAFETY: all file wrappers borrow the reader only for each call,
            // so they have dropped before the uniquely owned storage handle.
            unsafe {
                CascCloseStorage(self.0);
            }
        }
    }

    fn casc_error(operation: &'static str) -> Error {
        // SAFETY: GetCascError reads CascLib's thread-local numeric error and
        // requires no pointer or handle.
        let code = unsafe { GetCascError() };
        Error::Casc { operation, code }
    }

    fn casc_member_error(operation: &str, lookup_name: &str) -> Error {
        // SAFETY: GetCascError reads CascLib's thread-local numeric error and
        // requires no pointer or handle.
        let code = unsafe { GetCascError() };
        Error::Message(format!(
            "CascLib operation {operation} for {lookup_name} failed with error {code}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    struct FakeReader {
        files: BTreeMap<String, Vec<u8>>,
        requested: Vec<String>,
        fail_at: Option<usize>,
        writes: usize,
    }

    impl ArchiveReader for FakeReader {
        fn copy_named(&mut self, name: &str, destination: &mut dyn Write) -> Result<()> {
            self.requested.push(name.to_owned());
            if self.fail_at == Some(self.requested.len()) {
                return Err(Error::Message("injected read failure".to_owned()));
            }
            let bytes = self
                .files
                .get(name)
                .ok_or_else(|| Error::MissingInput(name.to_owned()))?;
            for chunk in bytes.chunks(2) {
                destination
                    .write_all(chunk)
                    .map_err(|source| error::io("fake destination", source))?;
                self.writes += 1;
            }
            Ok(())
        }
    }

    fn fake() -> FakeReader {
        FakeReader {
            files: SOURCE_WHITELIST
                .iter()
                .map(|name| ((*name).to_owned(), format!("fixture:{name}").into_bytes()))
                .collect(),
            requested: Vec::new(),
            fail_at: None,
            writes: 0,
        }
    }

    #[test]
    fn exporter_reads_only_whitelist_and_streams() {
        let directory = temporary_directory("whitelist");
        let output = directory.join("output.tar");
        let mut reader = fake();
        export_with_reader(&mut reader, b"build", &output).unwrap();
        assert_eq!(reader.requested, SOURCE_WHITELIST);
        assert!(reader.writes > SOURCE_WHITELIST.len());
        let names = tar::Archive::new(File::open(output).unwrap())
            .entries()
            .unwrap()
            .map(|entry| {
                entry
                    .unwrap()
                    .path()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(names[0], ".build.info");
        assert_eq!(&names[1..], SOURCE_WHITELIST);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn exporter_is_deterministic_and_cleans_every_injected_failure() {
        let directory = temporary_directory("failures");
        let first = directory.join("first.tar");
        let second = directory.join("second.tar");
        export_with_reader(&mut fake(), b"build", &first).unwrap();
        export_with_reader(&mut fake(), b"build", &second).unwrap();
        assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());

        for failure in 1..=SOURCE_WHITELIST.len() {
            let output = directory.join(format!("failure-{failure}.tar"));
            let mut reader = fake();
            reader.fail_at = Some(failure);
            assert!(export_with_reader(&mut reader, b"build", &output).is_err());
            assert!(!output.exists());
            assert!(fs::read_dir(&directory).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains("stage")
            }));
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn archive_names_reject_traversal_absolute_and_backslash() {
        assert!(validate_archive_name("../escape").is_err());
        assert!(validate_archive_name("/absolute").is_err());
        assert!(validate_archive_name("windows\\escape").is_err());
    }

    #[test]
    fn casc_lookup_adds_exact_prefix_after_validating_archive_name() {
        assert_eq!(
            casc_lookup_name("data/global/excel/armor.txt").unwrap(),
            "data:data/global/excel/armor.txt"
        );
        assert!(casc_lookup_name("../escape").is_err());
        assert!(casc_lookup_name("/absolute").is_err());
        assert!(casc_lookup_name("windows\\escape").is_err());
    }

    fn temporary_directory(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "arreat-data-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).unwrap();
        path
    }
}
