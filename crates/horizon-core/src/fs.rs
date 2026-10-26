//! The file system the game reads and writes through: `std::fs` on native builds, and an
//! in-memory tree in the browser build (WEB.md), where `std::fs` fails every call. The
//! page fills the tree with the prepared resources before the game starts ([`mount_archive`]);
//! what the game writes (options, saves) lives until the tab closes.
//!
//! `Path::exists`, `is_file` and `is_dir` go through `std::fs` too, so code calls
//! [`PathExt`]'s `exists_vfs`, `is_file_vfs` and `is_dir_vfs` instead.

#[cfg(not(target_arch = "wasm32"))]
pub use std::fs::*;

#[cfg(target_arch = "wasm32")]
pub use web::*;

use std::path::Path;

/// The game folder: the working directory, or `/horizon` in the browser build.
pub fn current_dir() -> std::io::Result<std::path::PathBuf> {
    #[cfg(target_arch = "wasm32")]
    return Ok(std::path::PathBuf::from(web::CURRENT_DIR));
    #[cfg(not(target_arch = "wasm32"))]
    std::env::current_dir()
}

/// The running executable, or `/horizon/horizon` in the browser build.
pub fn current_exe() -> std::io::Result<std::path::PathBuf> {
    #[cfg(target_arch = "wasm32")]
    return Ok(Path::new(web::CURRENT_DIR).join("horizon"));
    #[cfg(not(target_arch = "wasm32"))]
    std::env::current_exe()
}

/// The process id, for temporary file names: `std::process::id`, which panics in the
/// browser build ("no pids on this platform"), where one page is one game and 1 is used.
pub fn process_id() -> u32 {
    #[cfg(target_arch = "wasm32")]
    return 1;
    #[cfg(not(target_arch = "wasm32"))]
    std::process::id()
}

/// `exists`, `is_file` and `is_dir` that see the browser build's files.
pub trait PathExt {
    fn exists_vfs(&self) -> bool;
    fn is_file_vfs(&self) -> bool;
    fn is_dir_vfs(&self) -> bool;
}

impl PathExt for Path {
    fn exists_vfs(&self) -> bool {
        metadata(self).is_ok()
    }
    fn is_file_vfs(&self) -> bool {
        metadata(self).is_ok_and(|m| m.is_file())
    }
    fn is_dir_vfs(&self) -> bool {
        metadata(self).is_ok_and(|m| m.is_dir())
    }
}

impl PathExt for Metadata {
    fn exists_vfs(&self) -> bool {
        true
    }
    fn is_file_vfs(&self) -> bool {
        self.is_file()
    }
    fn is_dir_vfs(&self) -> bool {
        self.is_dir()
    }
}

impl PathExt for FileType {
    fn exists_vfs(&self) -> bool {
        true
    }
    fn is_file_vfs(&self) -> bool {
        self.is_file()
    }
    fn is_dir_vfs(&self) -> bool {
        self.is_dir()
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::io::{self, Read, Seek, SeekFrom, Write};
    use std::path::{Component, Path, PathBuf};
    use std::sync::{Arc, Mutex};

    pub use std::fs::{FileTimes, Permissions, TryLockError};

    #[derive(Clone)]
    enum Node {
        File(Arc<Vec<u8>>),
        Dir,
    }

    static TREE: Mutex<BTreeMap<String, Node>> = Mutex::new(BTreeMap::new());

    /// The folder relative paths start from (`std::env::current_dir` has none in the browser).
    pub const CURRENT_DIR: &str = "/horizon";

    fn key(path: &Path) -> String {
        let mut parts: Vec<String> = Vec::new();
        if path.is_relative() {
            parts.extend(
                CURRENT_DIR
                    .split('/')
                    .filter(|p| !p.is_empty())
                    .map(str::to_owned),
            );
        }
        for c in path.components() {
            match c {
                Component::Normal(p) => parts.push(p.to_string_lossy().into_owned()),
                Component::ParentDir => {
                    parts.pop();
                }
                Component::RootDir | Component::Prefix(_) | Component::CurDir => {}
            }
        }
        format!("/{}", parts.join("/"))
    }

    fn not_found(path: &Path) -> io::Error {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("{}: not found", path.display()),
        )
    }

    fn parent_key(key: &str) -> Option<String> {
        let i = key.rfind('/')?;
        Some(if i == 0 {
            "/".to_owned()
        } else {
            key[..i].to_owned()
        })
    }

    fn make_parents(tree: &mut BTreeMap<String, Node>, key: &str) {
        let mut k = key.to_owned();
        while let Some(p) = parent_key(&k) {
            if p == k {
                break;
            }
            tree.entry(p.clone()).or_insert(Node::Dir);
            if p == "/" {
                break;
            }
            k = p;
        }
    }

    fn put(path: &Path, data: Vec<u8>) {
        let k = key(path);
        let mut tree = TREE.lock().unwrap();
        make_parents(&mut tree, &k);
        tree.insert(k, Node::File(Arc::new(data)));
    }

    /// Adds one file (and its folders).
    pub fn install(path: &Path, data: Vec<u8>) {
        put(path, data);
    }

    /// Adds every file of a Horizon web archive under `root`: repeated records of a
    /// little-endian u32 path length, the UTF-8 path, a u32 data length and the data
    /// (`tools/web/pack.py`). The number of files added.
    pub fn mount_archive(root: &Path, archive: &[u8]) -> io::Result<usize> {
        let bad = || io::Error::new(io::ErrorKind::InvalidData, "truncated web archive");
        let mut at = 0usize;
        let mut count = 0;
        let u32_at = |at: &mut usize| -> io::Result<usize> {
            let b = archive.get(*at..*at + 4).ok_or_else(bad)?;
            *at += 4;
            Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        };
        while at < archive.len() {
            let n = u32_at(&mut at)?;
            let name = std::str::from_utf8(archive.get(at..at + n).ok_or_else(bad)?)
                .map_err(|_| bad())?
                .to_owned();
            at += n;
            let len = u32_at(&mut at)?;
            let data = archive.get(at..at + len).ok_or_else(bad)?.to_vec();
            at += len;
            put(&root.join(name), data);
            count += 1;
        }
        Ok(count)
    }

    pub fn exists<P: AsRef<Path>>(path: P) -> io::Result<bool> {
        Ok(TREE.lock().unwrap().contains_key(&key(path.as_ref())))
    }

    pub fn read<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
        let path = path.as_ref();
        match TREE.lock().unwrap().get(&key(path)) {
            Some(Node::File(d)) => Ok(d.as_ref().clone()),
            Some(Node::Dir) => Err(io::Error::new(io::ErrorKind::IsADirectory, "is a folder")),
            None => Err(not_found(path)),
        }
    }

    pub fn read_to_string<P: AsRef<Path>>(path: P) -> io::Result<String> {
        String::from_utf8(read(path)?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    pub fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> io::Result<()> {
        put(path.as_ref(), contents.as_ref().to_vec());
        Ok(())
    }

    pub fn create_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
        create_dir_all(path)
    }

    pub fn create_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
        let k = key(path.as_ref());
        let mut tree = TREE.lock().unwrap();
        if let Some(Node::File(_)) = tree.get(&k) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "a file is there",
            ));
        }
        make_parents(&mut tree, &k);
        tree.insert(k, Node::Dir);
        Ok(())
    }

    pub fn remove_file<P: AsRef<Path>>(path: P) -> io::Result<()> {
        let path = path.as_ref();
        match TREE.lock().unwrap().remove(&key(path)) {
            Some(_) => Ok(()),
            None => Err(not_found(path)),
        }
    }

    pub fn remove_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
        remove_dir_all(path)
    }

    pub fn remove_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
        let path = path.as_ref();
        let k = key(path);
        let prefix = format!("{}/", k.trim_end_matches('/'));
        let mut tree = TREE.lock().unwrap();
        if tree.remove(&k).is_none() {
            return Err(not_found(path));
        }
        tree.retain(|name, _| !name.starts_with(&prefix));
        Ok(())
    }

    pub fn rename<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<()> {
        let (from, to) = (from.as_ref(), to.as_ref());
        let (fk, tk) = (key(from), key(to));
        let mut tree = TREE.lock().unwrap();
        let node = tree.remove(&fk).ok_or_else(|| not_found(from))?;
        let prefix = format!("{}/", fk.trim_end_matches('/'));
        let moved: Vec<(String, Node)> = tree
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .map(|(k, v)| (format!("{tk}/{}", &k[prefix.len()..]), v.clone()))
            .collect();
        tree.retain(|k, _| !k.starts_with(&prefix));
        make_parents(&mut tree, &tk);
        tree.insert(tk, node);
        tree.extend(moved);
        Ok(())
    }

    pub fn copy<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<u64> {
        let data = read(from)?;
        let n = data.len() as u64;
        put(to.as_ref(), data);
        Ok(n)
    }

    pub fn hard_link<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<()> {
        copy(from, to).map(drop)
    }

    pub fn canonicalize<P: AsRef<Path>>(path: P) -> io::Result<PathBuf> {
        let path = path.as_ref();
        let k = key(path);
        if TREE.lock().unwrap().contains_key(&k) {
            Ok(PathBuf::from(k))
        } else {
            Err(not_found(path))
        }
    }

    pub fn read_link<P: AsRef<Path>>(path: P) -> io::Result<PathBuf> {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{}: not a link", path.as_ref().display()),
        ))
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct FileType {
        dir: bool,
    }
    impl FileType {
        pub fn is_file(&self) -> bool {
            !self.dir
        }
        pub fn is_dir(&self) -> bool {
            self.dir
        }
        pub fn is_symlink(&self) -> bool {
            false
        }
    }

    #[derive(Clone, Debug)]
    pub struct Metadata {
        dir: bool,
        len: u64,
    }
    impl Metadata {
        pub fn is_file(&self) -> bool {
            !self.dir
        }
        pub fn is_dir(&self) -> bool {
            self.dir
        }
        pub fn is_symlink(&self) -> bool {
            false
        }
        pub fn len(&self) -> u64 {
            self.len
        }
        pub fn file_type(&self) -> FileType {
            FileType { dir: self.dir }
        }
        pub fn modified(&self) -> io::Result<std::time::SystemTime> {
            Err(io::ErrorKind::Unsupported.into())
        }
        pub fn created(&self) -> io::Result<std::time::SystemTime> {
            Err(io::ErrorKind::Unsupported.into())
        }
        pub fn accessed(&self) -> io::Result<std::time::SystemTime> {
            Err(io::ErrorKind::Unsupported.into())
        }
    }

    pub fn metadata<P: AsRef<Path>>(path: P) -> io::Result<Metadata> {
        let path = path.as_ref();
        match TREE.lock().unwrap().get(&key(path)) {
            Some(Node::File(d)) => Ok(Metadata {
                dir: false,
                len: d.len() as u64,
            }),
            Some(Node::Dir) => Ok(Metadata { dir: true, len: 0 }),
            None => Err(not_found(path)),
        }
    }

    pub fn symlink_metadata<P: AsRef<Path>>(path: P) -> io::Result<Metadata> {
        metadata(path)
    }

    #[derive(Debug)]
    pub struct DirEntry {
        path: PathBuf,
        meta: Metadata,
    }
    impl DirEntry {
        pub fn path(&self) -> PathBuf {
            self.path.clone()
        }
        pub fn file_name(&self) -> OsString {
            self.path.file_name().unwrap_or_default().to_owned()
        }
        pub fn file_type(&self) -> io::Result<FileType> {
            Ok(self.meta.file_type())
        }
        pub fn metadata(&self) -> io::Result<Metadata> {
            Ok(self.meta.clone())
        }
    }

    pub struct ReadDir(std::vec::IntoIter<io::Result<DirEntry>>);
    impl Iterator for ReadDir {
        type Item = io::Result<DirEntry>;
        fn next(&mut self) -> Option<Self::Item> {
            self.0.next()
        }
    }

    pub fn read_dir<P: AsRef<Path>>(path: P) -> io::Result<ReadDir> {
        let path = path.as_ref();
        let k = key(path);
        let tree = TREE.lock().unwrap();
        match tree.get(&k) {
            Some(Node::Dir) => {}
            Some(Node::File(_)) => {
                return Err(io::Error::new(io::ErrorKind::NotADirectory, "a file"));
            }
            None => return Err(not_found(path)),
        }
        let prefix = if k == "/" {
            "/".to_owned()
        } else {
            format!("{k}/")
        };
        let entries: Vec<io::Result<DirEntry>> = tree
            .range(prefix.clone()..)
            .take_while(|(name, _)| name.starts_with(&prefix))
            .filter(|(name, _)| !name[prefix.len()..].contains('/'))
            .map(|(name, node)| {
                Ok(DirEntry {
                    path: path.join(&name[prefix.len()..]),
                    meta: match node {
                        Node::File(d) => Metadata {
                            dir: false,
                            len: d.len() as u64,
                        },
                        Node::Dir => Metadata { dir: true, len: 0 },
                    },
                })
            })
            .collect();
        Ok(ReadDir(entries.into_iter()))
    }

    /// A file open in memory: reads see its contents when opened, writes reach the tree when
    /// it is flushed or dropped.
    #[derive(Debug)]
    pub struct File {
        path: PathBuf,
        data: Vec<u8>,
        pos: usize,
        write: bool,
        dirty: bool,
    }

    impl File {
        pub fn open<P: AsRef<Path>>(path: P) -> io::Result<File> {
            OpenOptions::new().read(true).open(path)
        }
        pub fn create<P: AsRef<Path>>(path: P) -> io::Result<File> {
            OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)
        }
        pub fn create_new<P: AsRef<Path>>(path: P) -> io::Result<File> {
            OpenOptions::new().write(true).create_new(true).open(path)
        }
        pub fn options() -> OpenOptions {
            OpenOptions::new()
        }
        pub fn metadata(&self) -> io::Result<Metadata> {
            Ok(Metadata {
                dir: false,
                len: self.data.len() as u64,
            })
        }
        pub fn set_len(&mut self, size: u64) -> io::Result<()> {
            self.data.resize(size as usize, 0);
            self.dirty = true;
            Ok(())
        }
        pub fn sync_all(&self) -> io::Result<()> {
            self.commit();
            Ok(())
        }
        pub fn sync_data(&self) -> io::Result<()> {
            self.commit();
            Ok(())
        }
        pub fn try_clone(&self) -> io::Result<File> {
            self.commit();
            Ok(File {
                path: self.path.clone(),
                data: self.data.clone(),
                pos: self.pos,
                write: self.write,
                dirty: false,
            })
        }
        pub fn lock(&self) -> io::Result<()> {
            Ok(())
        }
        pub fn try_lock(&self) -> Result<(), std::fs::TryLockError> {
            Ok(())
        }
        pub fn unlock(&self) -> io::Result<()> {
            Ok(())
        }
        fn commit(&self) {
            if self.write {
                put(&self.path, self.data.clone());
            }
        }
    }

    impl Drop for File {
        fn drop(&mut self) {
            if self.dirty {
                self.commit();
            }
        }
    }

    impl Read for File {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let rest = self.data.get(self.pos..).unwrap_or_default();
            let n = rest.len().min(buf.len());
            buf[..n].copy_from_slice(&rest[..n]);
            self.pos += n;
            Ok(n)
        }
    }
    impl Read for &File {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::ErrorKind::Unsupported.into())
        }
    }

    impl Write for File {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if !self.write {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "opened for reading",
                ));
            }
            let end = self.pos + buf.len();
            if self.data.len() < end {
                self.data.resize(end, 0);
            }
            self.data[self.pos..end].copy_from_slice(buf);
            self.pos = end;
            self.dirty = true;
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            if self.dirty {
                self.commit();
                self.dirty = false;
            }
            Ok(())
        }
    }

    impl Seek for File {
        fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
            let pos = match to {
                SeekFrom::Start(n) => n as i64,
                SeekFrom::End(n) => self.data.len() as i64 + n,
                SeekFrom::Current(n) => self.pos as i64 + n,
            };
            if pos < 0 {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            self.pos = pos as usize;
            Ok(self.pos as u64)
        }
    }

    #[derive(Clone, Debug, Default)]
    pub struct OpenOptions {
        read: bool,
        write: bool,
        append: bool,
        truncate: bool,
        create: bool,
        create_new: bool,
    }

    impl OpenOptions {
        pub fn new() -> Self {
            Self::default()
        }
        pub fn read(&mut self, on: bool) -> &mut Self {
            self.read = on;
            self
        }
        pub fn write(&mut self, on: bool) -> &mut Self {
            self.write = on;
            self
        }
        pub fn append(&mut self, on: bool) -> &mut Self {
            self.append = on;
            self
        }
        pub fn truncate(&mut self, on: bool) -> &mut Self {
            self.truncate = on;
            self
        }
        pub fn create(&mut self, on: bool) -> &mut Self {
            self.create = on;
            self
        }
        pub fn create_new(&mut self, on: bool) -> &mut Self {
            self.create_new = on;
            self
        }
        pub fn open<P: AsRef<Path>>(&self, path: P) -> io::Result<File> {
            let path = path.as_ref();
            let existing = match TREE.lock().unwrap().get(&key(path)) {
                Some(Node::File(d)) => Some(d.as_ref().clone()),
                Some(Node::Dir) => {
                    return Err(io::Error::new(io::ErrorKind::IsADirectory, "is a folder"));
                }
                None => None,
            };
            let write = self.write || self.append;
            let data = match existing {
                Some(_) if self.create_new => {
                    return Err(io::ErrorKind::AlreadyExists.into());
                }
                Some(_) if self.truncate && write => Vec::new(),
                Some(d) => d,
                None if write && (self.create || self.create_new) => Vec::new(),
                None => return Err(not_found(path)),
            };
            let pos = if self.append { data.len() } else { 0 };
            let file = File {
                path: path.to_path_buf(),
                data,
                pos,
                write,
                dirty: write,
            };
            if write {
                file.commit();
            }
            Ok(file)
        }
    }
}
