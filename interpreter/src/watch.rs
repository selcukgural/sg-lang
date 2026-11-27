//! Watching folders, for the interpreter: what `Directory.watch`, `Directory.watchAll` and a
//! `DirectoryWatcher` do, the way compiled code does them (see `WATCH_RUNTIME` in `emit_c.rs`,
//! choices 2056-2065). Each watcher has a queue of changes, each a kind and a name under the
//! folder, which the system's side fills: on Linux a thread of its own that reads inotify, on
//! macOS FSEvents on a dispatch queue of its own, on Windows a thread of its own that waits for
//! `ReadDirectoryChangesExW`. The fiber that waits for a change is woken through the
//! scheduler's `Done`, as one waiting for a signal is.
//!
//! What every system is made to agree on: a rename is the old name deleted and the new one made;
//! a folder is never changed; a folder that comes in under `watchAll` (made or moved in) has
//! what is in it told as made, which closes the gap on Linux between a folder being made and its
//! watch being added; the folder itself is never told, only its end; the same change twice in a
//! row before it is taken is one; at most `MOST` changes wait, and beyond that one `Lost` of the
//! folder stands for those dropped.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use crate::net::Done;

pub const CREATED: u8 = 0;
pub const DELETED: u8 = 1;
pub const CHANGED: u8 = 2;
pub const LOST: u8 = 3;

/// The most changes a watcher keeps for the program (2061), as many as inotify keeps by default.
const MOST: usize = 16384;

/// The separator of the names in a change.
#[cfg(windows)]
const SEPARATOR: u8 = b'\\';
#[cfg(not(windows))]
const SEPARATOR: u8 = b'/';

struct Queue {
    /// The changes not taken yet: each a kind and the bytes of a name under the folder (WTF-8 on
    /// Windows), `""` for the folder itself.
    items: VecDeque<(u8, Vec<u8>)>,
    /// The folder itself is gone: once `items` is empty, `NotFound`.
    ended: bool,
    /// The fiber that waits for a change: the work it waits for, and whom to tell.
    waiter: Option<(u64, Arc<Done>)>,
}

/// What a watcher shares with the side of the system that fills it.
pub struct Shared {
    queue: Mutex<Queue>,
    came: Condvar,
}

impl Shared {
    fn new() -> Arc<Self> {
        Arc::new(Shared { queue: Mutex::new(Queue { items: VecDeque::new(), ended: false, waiter: None }), came: Condvar::new() })
    }

    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn wake(&self, queue: &mut Queue) {
        self.came.notify_all();
        if let Some((work, done)) = queue.waiter.take() {
            done.tell(work);
        }
    }

    /// A change, kept unless it is the last one again; beyond `MOST`, dropped, and a `Lost` of the
    /// folder says so once.
    pub fn push(&self, kind: u8, name: &[u8]) {
        let mut queue = self.lock();
        if queue.ended || queue.items.back().is_some_and(|(k, n)| *k == kind && n == name) {
            return;
        }
        if queue.items.len() >= MOST {
            if queue.items.back().is_some_and(|(k, n)| *k == LOST && n.is_empty()) {
                return;
            }
            queue.items.push_back((LOST, Vec::new()));
        } else {
            queue.items.push_back((kind, name.to_vec()));
        }
        self.wake(&mut queue);
    }

    /// The folder itself is gone.
    pub fn end(&self) {
        let mut queue = self.lock();
        queue.ended = true;
        self.wake(&mut queue);
    }
}

/// A name under the folder: `dir` and `name` joined, `name` alone under the folder itself.
fn join(dir: &[u8], name: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(dir.len() + 1 + name.len());
    if !dir.is_empty() {
        out.extend_from_slice(dir);
        out.push(SEPARATOR);
    }
    out.extend_from_slice(name);
    out
}

/// The path of a name under the folder, for the system.
fn full(root: &std::path::Path, rel: &[u8]) -> std::path::PathBuf {
    if rel.is_empty() {
        return root.to_path_buf();
    }
    // SAFETY: the bytes came from `as_encoded_bytes` of names of the system, joined by an ASCII
    // separator, so they are bytes such an `OsStr` has.
    root.join(unsafe { std::ffi::OsStr::from_encoded_bytes_unchecked(rel) })
}

/// The node of an entry of a folder, which tells one file from another at the same name; 0 on
/// Windows, where nothing asks for it.
#[cfg(unix)]
fn node(entry: &std::fs::DirEntry) -> u64 {
    std::os::unix::fs::DirEntryExt::ino(entry)
}

#[cfg(windows)]
fn node(_: &std::fs::DirEntry) -> u64 {
    0
}

/// What is in the folder `rel` and in every folder in it, each name with its node and whether it
/// is a folder handed to `on_item`, in the order of the bytes of the names in each folder;
/// `on_folder` is called for each folder found before what is in it is read (Linux adds its watch
/// there), and a folder it refuses is not gone into. A link is not followed. An error of
/// `on_folder` that is not to be passed over ends it.
fn scan(root: &std::path::Path, rel: &[u8], on_item: &mut dyn FnMut(&[u8], u64, bool), on_folder: &mut dyn FnMut(&[u8]) -> std::io::Result<bool>) -> std::io::Result<()> {
    let mut folders = vec![rel.to_vec()];
    while let Some(dir) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(full(root, &dir)) else { continue };
        let mut found: Vec<(Vec<u8>, u64, bool)> = entries
            .filter_map(Result::ok)
            .map(|entry| (entry.file_name().as_encoded_bytes().to_vec(), node(&entry), entry.file_type().is_ok_and(|kind| kind.is_dir())))
            .collect();
        found.sort();
        for (name, node, folder) in found {
            let child = join(&dir, &name);
            on_item(&child, node, folder);
            if folder && on_folder(&child)? {
                folders.push(child);
            }
        }
    }
    Ok(())
}

/// A watcher: what it shares, and the side of the system, which stops when it is dropped.
struct Watcher {
    shared: Arc<Shared>,
    _system: system::Side,
}

struct Table {
    next: u64,
    watchers: BTreeMap<u64, Arc<Watcher>>,
}

static TABLE: Mutex<Table> = Mutex::new(Table { next: 0, watchers: BTreeMap::new() });

fn table() -> MutexGuard<'static, Table> {
    TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn shared(id: u64) -> Arc<Shared> {
    table().watchers.get(&id).expect("a watcher lives while it is used").shared.clone()
}

/// A watcher of the folder, of every folder under it too with `all`; gives its number. A path
/// that is not a folder is `NotDirectory`.
pub fn start(path: &std::path::Path, all: bool) -> std::io::Result<u64> {
    if !std::fs::metadata(path)?.is_dir() {
        return Err(not_directory());
    }
    let shared = Shared::new();
    let side = system::Side::start(path, all, shared.clone())?;
    let mut table = table();
    table.next += 1;
    let id = table.next;
    table.watchers.insert(id, Arc::new(Watcher { shared, _system: side }));
    Ok(id)
}

#[cfg(unix)]
fn not_directory() -> std::io::Error {
    std::io::Error::from_raw_os_error(20)
}

#[cfg(windows)]
fn not_directory() -> std::io::Error {
    std::io::Error::from_raw_os_error(267)
}

/// The watcher ends: the side of the system stops before this returns.
pub fn stop(id: u64) {
    let watcher = table().watchers.remove(&id);
    drop(watcher);
}

/// Whether the watcher keeps a change or its folder is gone; when not, the work `work` is told
/// through `done` once it does.
pub fn ready_or_wait(id: u64, work: u64, done: Arc<Done>) -> bool {
    let shared = shared(id);
    let mut queue = shared.lock();
    if !queue.items.is_empty() || queue.ended {
        return true;
    }
    queue.waiter = Some((work, done));
    false
}

/// A wait cut short by a cancel: nobody is to be told.
pub fn forget_waiter(id: u64) {
    if let Some(watcher) = table().watchers.get(&id) {
        watcher.shared.lock().waiter = None;
    }
}

/// Every change the watcher keeps, taken; `None` when there is none and the folder is gone.
pub fn take(id: u64) -> Option<Vec<(u8, Vec<u8>)>> {
    let shared = shared(id);
    let mut queue = shared.lock();
    if queue.items.is_empty() && queue.ended {
        return None;
    }
    Some(queue.items.drain(..).collect())
}

#[cfg(target_os = "linux")]
mod system {
    //! inotify: a watch of each folder, a thread that reads what it tells.
    use super::{join, scan, Shared, CHANGED, CREATED, DELETED, LOST};
    use std::collections::HashMap;
    use std::ffi::{c_char, c_int, c_void, CString};
    use std::os::unix::ffi::OsStrExt;
    use std::sync::Arc;

    extern "C" {
        fn inotify_init1(flags: c_int) -> c_int;
        fn inotify_add_watch(fd: c_int, path: *const c_char, mask: u32) -> c_int;
        fn inotify_rm_watch(fd: c_int, wd: c_int) -> c_int;
        fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
        fn write(fd: c_int, buf: *const c_void, count: usize) -> isize;
        fn close(fd: c_int) -> c_int;
        fn pipe2(fds: *mut c_int, flags: c_int) -> c_int;
    }

    const CLOEXEC: c_int = 0o2000000;
    const NONBLOCK: c_int = 0o4000;
    const IN_MODIFY: u32 = 0x2;
    const IN_ATTRIB: u32 = 0x4;
    const IN_MOVED_FROM: u32 = 0x40;
    const IN_MOVED_TO: u32 = 0x80;
    const IN_CREATE: u32 = 0x100;
    const IN_DELETE: u32 = 0x200;
    const IN_DELETE_SELF: u32 = 0x400;
    const IN_MOVE_SELF: u32 = 0x800;
    const IN_UNMOUNT: u32 = 0x2000;
    const IN_Q_OVERFLOW: u32 = 0x4000;
    const IN_IGNORED: u32 = 0x8000;
    const IN_ONLYDIR: u32 = 0x0100_0000;
    const IN_DONT_FOLLOW: u32 = 0x0200_0000;
    const IN_EXCL_UNLINK: u32 = 0x0400_0000;
    const IN_ISDIR: u32 = 0x4000_0000;
    /// What each folder is watched for.
    const MASK: u32 = IN_MODIFY | IN_ATTRIB | IN_MOVED_FROM | IN_MOVED_TO | IN_CREATE | IN_DELETE | IN_DELETE_SELF | IN_MOVE_SELF | IN_ONLYDIR | IN_EXCL_UNLINK;
    const ENOSPC: i32 = 28;

    /// The watches: the folder under the watched one each is of.
    struct Watches {
        fd: c_int,
        root: std::path::PathBuf,
        root_wd: c_int,
        all: bool,
        folders: HashMap<c_int, Vec<u8>>,
    }

    impl Watches {
        /// A watch of the folder under the watched one; a link is not followed.
        fn add(&mut self, rel: &[u8]) -> std::io::Result<()> {
            let path = CString::new(super::full(&self.root, rel).as_os_str().as_bytes()).map_err(|_| std::io::Error::from_raw_os_error(22))?;
            // SAFETY: a path ended by a zero.
            let wd = unsafe { inotify_add_watch(self.fd, path.as_ptr(), MASK | IN_DONT_FOLLOW) };
            if wd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            self.folders.insert(wd, rel.to_vec());
            Ok(())
        }

        /// The watches of the folder and of those under it go: it was moved away.
        fn forget(&mut self, rel: &[u8]) {
            let under: Vec<c_int> = self
                .folders
                .iter()
                .filter(|(_, folder)| folder.as_slice() == rel || (folder.starts_with(rel) && folder.get(rel.len()) == Some(&b'/')))
                .map(|(wd, _)| *wd)
                .collect();
            for wd in under {
                self.folders.remove(&wd);
                // SAFETY: a watch of this inotify.
                unsafe { inotify_rm_watch(self.fd, wd) };
            }
        }

        /// A folder came in: watched, and what is in it told as made. A watch the system has no
        /// room for is a `Lost` of the folder.
        fn came_in(&mut self, rel: &[u8], shared: &Shared) {
            match self.add(rel) {
                Ok(()) => {}
                Err(error) => {
                    if error.raw_os_error() == Some(ENOSPC) {
                        shared.push(LOST, rel);
                    }
                    return;
                }
            }
            let root = self.root.clone();
            let _ = scan(&root, rel, &mut |child, _, _| shared.push(CREATED, child), &mut |child| match self.add(child) {
                Ok(()) => Ok(true),
                Err(error) => {
                    if error.raw_os_error() == Some(ENOSPC) {
                        shared.push(LOST, child);
                    }
                    Ok(false)
                }
            });
        }

        /// One event; `true` when the folder itself is gone.
        fn event(&mut self, wd: c_int, mask: u32, name: &[u8], shared: &Shared) -> bool {
            if mask & IN_Q_OVERFLOW != 0 {
                shared.push(LOST, b"");
                return false;
            }
            if mask & IN_IGNORED != 0 {
                self.folders.remove(&wd);
                return wd == self.root_wd;
            }
            if mask & (IN_DELETE_SELF | IN_MOVE_SELF | IN_UNMOUNT) != 0 {
                return wd == self.root_wd;
            }
            let Some(dir) = self.folders.get(&wd) else { return false };
            if name.is_empty() {
                return false;
            }
            let rel = join(dir, name);
            let folder = mask & IN_ISDIR != 0;
            if mask & (IN_CREATE | IN_MOVED_TO) != 0 {
                shared.push(CREATED, &rel);
                if folder && self.all {
                    self.came_in(&rel, shared);
                }
            } else if mask & (IN_DELETE | IN_MOVED_FROM) != 0 {
                shared.push(DELETED, &rel);
                if folder && self.all && mask & IN_MOVED_FROM != 0 {
                    self.forget(&rel);
                }
            } else if mask & (IN_MODIFY | IN_ATTRIB) != 0 && !folder {
                shared.push(CHANGED, &rel);
            }
            false
        }
    }

    /// The side of the system: the end of the pipe that stops the thread, and the thread.
    pub struct Side {
        stop: c_int,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Side {
        pub fn start(path: &std::path::Path, all: bool, shared: Arc<Shared>) -> std::io::Result<Side> {
            // SAFETY: plain calls of the system.
            let fd = unsafe { inotify_init1(NONBLOCK | CLOEXEC) };
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let mut watches = Watches { fd, root: path.to_path_buf(), root_wd: -1, all, folders: HashMap::new() };
            let started = (|| {
                let root = CString::new(path.as_os_str().as_bytes()).map_err(|_| std::io::Error::from_raw_os_error(22))?;
                // SAFETY: a path ended by a zero. The folder itself is followed if it is a link.
                let wd = unsafe { inotify_add_watch(fd, root.as_ptr(), MASK) };
                if wd < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                watches.root_wd = wd;
                watches.folders.insert(wd, Vec::new());
                if all {
                    // Every folder under it watched before the call returns; one the system has
                    // no room for is an error of the call.
                    let root = watches.root.clone();
                    scan(&root, b"", &mut |_, _, _| {}, &mut |child| match watches.add(child) {
                        Ok(()) => Ok(true),
                        Err(error) if error.raw_os_error() == Some(ENOSPC) => Err(error),
                        Err(_) => Ok(false),
                    })?;
                }
                let mut ends = [0 as c_int; 2];
                // SAFETY: room for the two ends.
                if unsafe { pipe2(ends.as_mut_ptr(), NONBLOCK | CLOEXEC) } != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(ends)
            })();
            let ends = match started {
                Ok(ends) => ends,
                Err(error) => {
                    // SAFETY: the inotify made here.
                    unsafe { close(fd) };
                    return Err(error);
                }
            };
            let heard = ends[0];
            let thread = std::thread::Builder::new().name("watch".to_string()).spawn(move || run(watches, heard, &shared));
            match thread {
                Ok(thread) => Ok(Side { stop: ends[1], thread: Some(thread) }),
                Err(error) => {
                    // SAFETY: the ends and the inotify made here.
                    unsafe {
                        close(ends[0]);
                        close(ends[1]);
                        close(fd);
                    }
                    Err(error)
                }
            }
        }
    }

    impl Drop for Side {
        fn drop(&mut self) {
            // SAFETY: one byte into the pipe the thread waits on.
            unsafe { write(self.stop, [1u8].as_ptr().cast(), 1) };
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
            // SAFETY: the end made in `start`; the thread closed its own.
            unsafe { close(self.stop) };
        }
    }

    /// The thread: it reads what inotify tells until the pipe says stop, or the folder is gone.
    fn run(mut watches: Watches, heard: c_int, shared: &Shared) {
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let ready = crate::net::wait(&[(watches.fd, false), (heard, false)], None);
            if ready[1] {
                break;
            }
            // SAFETY: `buffer` has room for its length.
            let got = unsafe { read(watches.fd, buffer.as_mut_ptr().cast(), buffer.len()) };
            if got <= 0 {
                continue;
            }
            let got = got as usize;
            let mut at = 0;
            let mut gone = false;
            while at + 16 <= got {
                let field = |i: usize| u32::from_ne_bytes(buffer[at + i..at + i + 4].try_into().expect("four bytes"));
                let (wd, mask, len) = (field(0) as c_int, field(4), field(12) as usize);
                let name = &buffer[at + 16..(at + 16 + len).min(got)];
                let name = &name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())];
                let name = name.to_vec();
                gone |= watches.event(wd, mask, &name, shared);
                at += 16 + len;
            }
            if gone {
                shared.end();
                break;
            }
        }
        // SAFETY: the inotify and the end of the pipe this thread was given.
        unsafe {
            close(watches.fd);
            close(heard);
        }
    }
}

#[cfg(target_os = "macos")]
mod system {
    //! FSEvents: a stream of the events of the files under the folder, told on a dispatch queue
    //! of its own. Its functions are taken by name from CoreServices the first time, so a
    //! program that watches nothing loads nothing more.
    use super::{scan, Shared, CHANGED, CREATED, DELETED, LOST};
    use std::collections::HashMap;
    use std::ffi::{c_char, c_int, c_void, CStr, CString};
    use std::os::unix::ffi::OsStrExt;
    use std::sync::{Arc, Mutex, OnceLock};

    extern "C" {
        fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
        fn dispatch_queue_create(label: *const c_char, attr: *const c_void) -> *mut c_void;
        fn dispatch_sync_f(queue: *mut c_void, context: *mut c_void, work: extern "C" fn(*mut c_void));
        fn dispatch_release(object: *mut c_void);
        fn realpath(path: *const c_char, resolved: *mut c_char) -> *mut c_char;
        fn free(block: *mut c_void);
    }

    type Callback = extern "C" fn(*const c_void, *mut c_void, usize, *mut c_void, *const u32, *const u64);

    #[repr(C)]
    struct Context {
        version: isize,
        info: *mut c_void,
        retain: *const c_void,
        release: *const c_void,
        describe: *const c_void,
    }

    type Create = unsafe extern "C" fn(*const c_void, Callback, *const Context, *const c_void, u64, f64, u32) -> *mut c_void;
    type OfStream = unsafe extern "C" fn(*mut c_void);
    type SetQueue = unsafe extern "C" fn(*mut c_void, *mut c_void);
    type Start = unsafe extern "C" fn(*mut c_void) -> u8;
    type MakeString = unsafe extern "C" fn(*const c_void, *const c_char) -> *const c_void;
    type MakeArray = unsafe extern "C" fn(*const c_void, *const *const c_void, isize, *const c_void) -> *const c_void;
    type Release = unsafe extern "C" fn(*const c_void);

    struct Functions {
        create: Create,
        set_queue: SetQueue,
        start: Start,
        stop: OfStream,
        invalidate: OfStream,
        release: OfStream,
        string: MakeString,
        array: MakeArray,
        cf_release: Release,
        callbacks: *const c_void,
    }

    // SAFETY: addresses of functions and of a constant of the system, the same on every thread.
    unsafe impl Send for Functions {}
    unsafe impl Sync for Functions {}

    fn functions() -> Option<&'static Functions> {
        static FUNCTIONS: OnceLock<Option<Functions>> = OnceLock::new();
        FUNCTIONS
            .get_or_init(|| {
                const NOW: c_int = 2;
                // SAFETY: libraries of the system, opened once and never closed; each address is
                // that of the function the type says (CoreServices' headers).
                unsafe {
                    let services = dlopen(c"/System/Library/Frameworks/CoreServices.framework/CoreServices".as_ptr(), NOW);
                    let foundation = dlopen(c"/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation".as_ptr(), NOW);
                    if services.is_null() || foundation.is_null() {
                        return None;
                    }
                    let find = |handle: *mut c_void, name: &CStr| {
                        let found = dlsym(handle, name.as_ptr());
                        (!found.is_null()).then_some(found)
                    };
                    Some(Functions {
                        create: std::mem::transmute::<*mut c_void, Create>(find(services, c"FSEventStreamCreate")?),
                        set_queue: std::mem::transmute::<*mut c_void, SetQueue>(find(services, c"FSEventStreamSetDispatchQueue")?),
                        start: std::mem::transmute::<*mut c_void, Start>(find(services, c"FSEventStreamStart")?),
                        stop: std::mem::transmute::<*mut c_void, OfStream>(find(services, c"FSEventStreamStop")?),
                        invalidate: std::mem::transmute::<*mut c_void, OfStream>(find(services, c"FSEventStreamInvalidate")?),
                        release: std::mem::transmute::<*mut c_void, OfStream>(find(services, c"FSEventStreamRelease")?),
                        string: std::mem::transmute::<*mut c_void, MakeString>(find(foundation, c"CFStringCreateWithFileSystemRepresentation")?),
                        array: std::mem::transmute::<*mut c_void, MakeArray>(find(foundation, c"CFArrayCreate")?),
                        cf_release: std::mem::transmute::<*mut c_void, Release>(find(foundation, c"CFRelease")?),
                        callbacks: find(foundation, c"kCFTypeArrayCallBacks")?,
                    })
                }
            })
            .as_ref()
    }

    const SINCE_NOW: u64 = u64::MAX;
    const NO_DEFER: u32 = 0x02;
    const WATCH_ROOT: u32 = 0x04;
    const FILE_EVENTS: u32 = 0x10;
    const MUST_SCAN: u32 = 0x01;
    const USER_DROPPED: u32 = 0x02;
    const KERNEL_DROPPED: u32 = 0x04;
    const ROOT_CHANGED: u32 = 0x20;
    const INODE_META: u32 = 0x400;
    const MODIFIED: u32 = 0x1000;
    const FINDER_INFO: u32 = 0x2000;
    const OWNER: u32 = 0x4000;
    const XATTR: u32 = 0x8000;
    /// How long FSEvents gathers events before it tells them, in seconds; the first after a
    /// quiet time is told at once (`NO_DEFER`).
    const LATENCY: f64 = 0.01;

    /// What the callback is given: the queue to fill and how to read the paths.
    struct Watch {
        shared: Arc<Shared>,
        /// The folder as FSEvents names it: its real path, no link in it.
        real: Vec<u8>,
        /// The folder as it was given, and its device and node, to tell when it is gone.
        root: std::path::PathBuf,
        node: (u64, u64),
        all: bool,
        /// Set before the stream stops: nothing more is kept.
        stopped: Mutex<bool>,
        /// The names under the folder as the watcher last knew them, each with its node and
        /// whether it is a folder: FSEvents gathers the flags of a file over several seconds,
        /// before the watch too, so what changed is told by these and the disk, and the flags
        /// only tell a file written. Used on the queue of the stream alone.
        known: Mutex<HashMap<Vec<u8>, (u64, bool)>>,
    }

    fn node_of(path: &std::path::Path) -> Option<(u64, u64)> {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path).ok().map(|info| (info.dev(), info.ino()))
    }

    /// The name is no longer known; with `under`, nor any under it (`""`: every name).
    fn forget(known: &mut HashMap<Vec<u8>, (u64, bool)>, rel: &[u8], under: bool) {
        known.remove(rel);
        if under {
            known.retain(|name, _| !(rel.is_empty() || (name.len() > rel.len() && name[rel.len()] == b'/' && name.starts_with(rel))));
        }
    }

    impl Watch {
        fn learn(&self, known: &mut HashMap<Vec<u8>, (u64, bool)>, rel: &[u8]) {
            let all = self.all;
            let _ = scan(&self.root, rel, &mut |child, node, folder| {
                known.insert(child.to_vec(), (node, folder));
            }, &mut |_| Ok(all));
        }

        fn event(&self, path: &[u8], flags: u32) {
            if flags & ROOT_CHANGED != 0 {
                if node_of(&self.root) != Some(self.node) {
                    self.shared.end();
                }
                return;
            }
            let mut path = path;
            while path.len() > 1 && path.ends_with(b"/") {
                path = &path[..path.len() - 1];
            }
            let rel = match path.strip_prefix(self.real.as_slice()) {
                Some([]) => Some(&[][..]),
                Some([b'/', rest @ ..]) => Some(rest),
                _ => None,
            };
            let mut known = self.known.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if flags & (MUST_SCAN | USER_DROPPED | KERNEL_DROPPED) != 0 {
                let rel = rel.unwrap_or(&[]);
                self.shared.push(LOST, rel);
                // What is under it is learnt again, as the program will list it.
                forget(&mut known, rel, true);
                self.learn(&mut known, rel);
                return;
            }
            let Some(rel) = rel else { return };
            if rel.is_empty() || (!self.all && rel.contains(&b'/')) {
                return;
            }
            let was = known.get(rel).copied();
            let here = std::path::Path::new(std::ffi::OsStr::from_bytes(path));
            let Ok(info) = std::fs::symlink_metadata(here) else {
                if let Some((_, folder)) = was {
                    self.shared.push(DELETED, rel);
                    forget(&mut known, rel, folder);
                }
                return;
            };
            use std::os::unix::fs::MetadataExt;
            let folder = info.is_dir();
            if was.is_some_and(|(node, _)| node == info.ino()) {
                if !folder && flags & (MODIFIED | INODE_META | FINDER_INFO | OWNER | XATTR) != 0 {
                    self.shared.push(CHANGED, rel);
                }
                return;
            }
            // Another file at the name: the one before went.
            if let Some((_, before)) = was {
                self.shared.push(DELETED, rel);
                forget(&mut known, rel, before);
            }
            self.shared.push(CREATED, rel);
            known.insert(rel.to_vec(), (info.ino(), folder));
            if folder && self.all {
                let shared = &self.shared;
                let _ = scan(&self.root, rel, &mut |child, node, folder| {
                    if known.get(child).is_some_and(|(n, _)| *n == node) {
                        return;
                    }
                    shared.push(CREATED, child);
                    known.insert(child.to_vec(), (node, folder));
                }, &mut |_| Ok(true));
            }
        }
    }

    extern "C" fn told(_stream: *const c_void, info: *mut c_void, count: usize, paths: *mut c_void, flags: *const u32, _ids: *const u64) {
        // SAFETY: `info` is the `Watch` the stream was made with, which lives until the stream is
        // released; FSEvents gives `count` paths ended by a zero and `count` flags.
        let watch = unsafe { &*(info as *const Watch) };
        if *watch.stopped.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) {
            return;
        }
        let paths = paths as *const *const c_char;
        for i in 0..count {
            // SAFETY: as above.
            let (path, flag) = unsafe { (CStr::from_ptr(*paths.add(i)).to_bytes(), *flags.add(i)) };
            watch.event(path, flag);
        }
    }

    extern "C" fn nothing(_: *mut c_void) {}

    pub struct Side {
        stream: *mut c_void,
        queue: *mut c_void,
        watch: *mut Watch,
    }

    // SAFETY: the stream and the queue are used from any thread, as FSEvents allows; the `Watch`
    // is shared behind its locks.
    unsafe impl Send for Side {}
    unsafe impl Sync for Side {}

    impl Side {
        pub fn start(path: &std::path::Path, all: bool, shared: Arc<Shared>) -> std::io::Result<Side> {
            let Some(f) = functions() else { return Err(std::io::Error::other("FSEvents could not be loaded")) };
            let given = CString::new(path.as_os_str().as_bytes()).map_err(|_| std::io::Error::from_raw_os_error(22))?;
            // SAFETY: a path ended by a zero; `realpath` gives a block of `malloc`, freed here.
            let real = unsafe {
                let found = realpath(given.as_ptr(), std::ptr::null_mut());
                if found.is_null() {
                    return Err(std::io::Error::last_os_error());
                }
                let bytes = CStr::from_ptr(found).to_bytes().to_vec();
                free(found.cast());
                bytes
            };
            let node = node_of(path).ok_or_else(std::io::Error::last_os_error)?;
            let watch = Watch { shared, real: real.clone(), root: path.to_path_buf(), node, all, stopped: Mutex::new(false), known: Mutex::new(HashMap::new()) };
            // What is there now is known before the stream starts: an event of it is no change.
            {
                let mut known = HashMap::new();
                watch.learn(&mut known, b"");
                *watch.known.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = known;
            }
            let watch = Box::into_raw(Box::new(watch));
            let real = CString::new(real).expect("a path of the system has no zero byte");
            // SAFETY: CoreFoundation's own values, released once the stream holds them.
            unsafe {
                let name = (f.string)(std::ptr::null(), real.as_ptr());
                let paths = (f.array)(std::ptr::null(), &name, 1, f.callbacks);
                let context = Context { version: 0, info: watch.cast(), retain: std::ptr::null(), release: std::ptr::null(), describe: std::ptr::null() };
                let stream = (f.create)(std::ptr::null(), told, &context, paths, SINCE_NOW, LATENCY, NO_DEFER | WATCH_ROOT | FILE_EVENTS);
                (f.cf_release)(paths);
                (f.cf_release)(name);
                if stream.is_null() {
                    drop(Box::from_raw(watch));
                    return Err(std::io::Error::other("FSEvents made no stream"));
                }
                let queue = dispatch_queue_create(c"sg.watch".as_ptr(), std::ptr::null());
                (f.set_queue)(stream, queue);
                if (f.start)(stream) == 0 {
                    (f.invalidate)(stream);
                    (f.release)(stream);
                    dispatch_release(queue);
                    drop(Box::from_raw(watch));
                    return Err(std::io::Error::other("FSEvents did not start the stream"));
                }
                Ok(Side { stream, queue, watch })
            }
        }
    }

    impl Drop for Side {
        fn drop(&mut self) {
            let f = functions().expect("loaded when the stream was made");
            // SAFETY: the stream, the queue and the `Watch` made in `start`; once the queue has
            // run what it had, the callback runs no more, and the `Watch` can go.
            unsafe {
                *(*self.watch).stopped.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
                (f.stop)(self.stream);
                (f.invalidate)(self.stream);
                dispatch_sync_f(self.queue, std::ptr::null_mut(), nothing);
                (f.release)(self.stream);
                dispatch_release(self.queue);
                drop(Box::from_raw(self.watch));
            }
        }
    }
}

#[cfg(windows)]
mod system {
    //! `ReadDirectoryChangesExW` on a handle of the folder, waited for by a thread of its own.
    use super::{full, scan, Shared, CHANGED, CREATED, DELETED, LOST};
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::{AsRawHandle, RawHandle};
    use std::sync::Arc;

    type Handle = *mut c_void;

    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u64,
        event: Handle,
    }

    type ReadChanges = unsafe extern "system" fn(Handle, *mut c_void, u32, i32, u32, *mut u32, *mut Overlapped, *const c_void, i32) -> i32;

    extern "system" {
        fn CreateEventW(attributes: *const c_void, manual: i32, initial: i32, name: *const u16) -> Handle;
        fn SetEvent(event: Handle) -> i32;
        fn ResetEvent(event: Handle) -> i32;
        fn WaitForMultipleObjects(count: u32, handles: *const Handle, all: i32, ms: u32) -> u32;
        fn GetOverlappedResult(file: Handle, overlapped: *mut Overlapped, got: *mut u32, wait: i32) -> i32;
        fn CancelIoEx(file: Handle, overlapped: *mut Overlapped) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
        fn GetLastError() -> u32;
        fn GetModuleHandleW(name: *const u16) -> Handle;
        fn GetProcAddress(module: Handle, name: *const std::ffi::c_char) -> *mut c_void;
        fn GetFinalPathNameByHandleW(file: Handle, path: *mut u16, size: u32, flags: u32) -> u32;
    }

    const FILE_LIST_DIRECTORY: u32 = 1;
    const BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const OVERLAPPED: u32 = 0x4000_0000;
    const FILE_NAME: u32 = 0x1;
    const DIR_NAME: u32 = 0x2;
    const ATTRIBUTES: u32 = 0x4;
    const SIZE: u32 = 0x8;
    const LAST_WRITE: u32 = 0x10;
    const EXTENDED: i32 = 2;
    const ADDED: u32 = 1;
    const REMOVED: u32 = 2;
    const MODIFIED: u32 = 3;
    const OLD_NAME: u32 = 4;
    const NEW_NAME: u32 = 5;
    const ATTRIBUTE_DIRECTORY: u32 = 0x10;
    const ATTRIBUTE_REPARSE: u32 = 0x400;
    const NOTIFY_ENUM_DIR: u32 = 1022;
    const OPERATION_ABORTED: u32 = 995;
    const INFINITE: u32 = u32::MAX;
    /// The block the changes are told into: 64 KB, the most a folder of the network takes.
    const BLOCK: usize = 64 * 1024;

    fn read_changes() -> Option<ReadChanges> {
        let kernel: Vec<u16> = "kernel32.dll\0".encode_utf16().collect();
        // SAFETY: kernel32 is loaded in every program; the address is that of the function the
        // type says (Windows 10 1709 and later).
        unsafe {
            let found = GetProcAddress(GetModuleHandleW(kernel.as_ptr()), c"ReadDirectoryChangesExW".as_ptr());
            (!found.is_null()).then(|| std::mem::transmute::<*mut c_void, ReadChanges>(found))
        }
    }

    /// The final path of the open folder, to tell when it was moved away.
    fn final_path(handle: Handle) -> Vec<u16> {
        let mut path = vec![0u16; 1024];
        loop {
            // SAFETY: room for `path.len()` characters.
            let got = unsafe { GetFinalPathNameByHandleW(handle, path.as_mut_ptr(), path.len() as u32, 0) } as usize;
            if got == 0 {
                return Vec::new();
            }
            if got < path.len() {
                path.truncate(got);
                return path;
            }
            path.resize(got + 1, 0);
        }
    }

    struct Raw(Handle);
    // SAFETY: handles of the system, used from the thread they are given to.
    unsafe impl Send for Raw {}

    pub struct Side {
        stop: Handle,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    // SAFETY: an event of the system, set from any thread.
    unsafe impl Send for Side {}
    unsafe impl Sync for Side {}

    impl Side {
        pub fn start(path: &std::path::Path, all: bool, shared: Arc<Shared>) -> std::io::Result<Side> {
            let Some(read) = read_changes() else { return Err(std::io::Error::other("this Windows has no ReadDirectoryChangesExW")) };
            let folder = std::fs::OpenOptions::new().access_mode(FILE_LIST_DIRECTORY).share_mode(7).custom_flags(BACKUP_SEMANTICS | OVERLAPPED).open(path)?;
            // SAFETY: plain calls of the system.
            let (stop, done) = unsafe { (CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()), CreateEventW(std::ptr::null(), 1, 0, std::ptr::null())) };
            if stop.is_null() || done.is_null() {
                return Err(std::io::Error::last_os_error());
            }
            let first = final_path(folder.as_raw_handle());
            let root = path.to_path_buf();
            let (stop_here, done_here) = (Raw(stop), Raw(done));
            // Windows keeps the changes from the first call on, so the watch starts once the
            // thread has made it.
            let (armed, is_armed) = std::sync::mpsc::channel();
            let thread = std::thread::Builder::new().name("watch".to_string()).spawn(move || {
                let (stop, done) = (stop_here, done_here);
                run(folder, read, all, &root, first, stop.0, done.0, &shared, armed);
                // SAFETY: the event made for this thread.
                unsafe { CloseHandle(done.0) };
            })?;
            let _ = is_armed.recv();
            Ok(Side { stop, thread: Some(thread) })
        }
    }

    impl Drop for Side {
        fn drop(&mut self) {
            // SAFETY: the event the thread waits on.
            unsafe { SetEvent(self.stop) };
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
            // SAFETY: made in `start`.
            unsafe { CloseHandle(self.stop) };
        }
    }

    /// Whether the path is a folder and no link (a symbolic link or a junction).
    fn plain_folder(path: &std::path::Path) -> bool {
        use std::os::windows::fs::MetadataExt;
        std::fs::symlink_metadata(path).is_ok_and(|info| info.file_attributes() & ATTRIBUTE_DIRECTORY != 0 && info.file_attributes() & ATTRIBUTE_REPARSE == 0)
    }

    #[allow(clippy::too_many_arguments)]
    fn run(folder: std::fs::File, read: ReadChanges, all: bool, root: &std::path::Path, first: Vec<u16>, stop: Handle, done: Handle, shared: &Shared, armed: std::sync::mpsc::Sender<()>) {
        let mut armed = Some(armed);
        let handle: RawHandle = folder.as_raw_handle();
        let mut block = vec![0u64; BLOCK / 8];
        let filter = FILE_NAME | DIR_NAME | ATTRIBUTES | SIZE | LAST_WRITE;
        loop {
            let mut overlapped = Overlapped { internal: 0, internal_high: 0, offset: 0, event: done };
            // SAFETY: the block and the `OVERLAPPED` live until the call is over, waited for
            // below in every case.
            unsafe { ResetEvent(done) };
            let asked = unsafe { read(handle, block.as_mut_ptr().cast(), BLOCK as u32, i32::from(all), filter, std::ptr::null_mut(), &mut overlapped, std::ptr::null(), EXTENDED) };
            if let Some(armed) = armed.take() {
                let _ = armed.send(());
            }
            if asked == 0 {
                shared.end();
                return;
            }
            let handles = [done, stop];
            // SAFETY: two events.
            let which = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) };
            let mut got = 0u32;
            if which == 1 {
                // SAFETY: the call above, cut off and waited for.
                unsafe {
                    CancelIoEx(handle, &mut overlapped);
                    GetOverlappedResult(handle, &mut overlapped, &mut got, 1);
                }
                return;
            }
            // SAFETY: the call is over.
            let ok = unsafe { GetOverlappedResult(handle, &mut overlapped, &mut got, 1) };
            if ok == 0 {
                // SAFETY: plain call.
                match unsafe { GetLastError() } {
                    NOTIFY_ENUM_DIR => {
                        shared.push(LOST, b"");
                        continue;
                    }
                    OPERATION_ABORTED => return,
                    _ => {
                        shared.end();
                        return;
                    }
                }
            }
            if got == 0 {
                shared.push(LOST, b"");
                continue;
            }
            // SAFETY: the system wrote `got` bytes into the block.
            let bytes = unsafe { std::slice::from_raw_parts(block.as_ptr().cast::<u8>(), got as usize) };
            let mut at = 0usize;
            loop {
                let word = |i: usize| u32::from_ne_bytes(bytes[at + i..at + i + 4].try_into().expect("four bytes"));
                let (next, action, attributes, length) = (word(0) as usize, word(4), word(56), word(80) as usize);
                let name: Vec<u16> = bytes[at + 84..at + 84 + length].as_chunks::<2>().0.iter().map(|pair| u16::from_ne_bytes(*pair)).collect();
                let rel = std::ffi::OsString::from_wide(&name).as_encoded_bytes().to_vec();
                let folder = attributes & ATTRIBUTE_DIRECTORY != 0 && attributes & ATTRIBUTE_REPARSE == 0;
                match action {
                    ADDED | NEW_NAME => {
                        shared.push(CREATED, &rel);
                        // A link made as a folder is told before it is a link: what it is now
                        // decides.
                        if folder && all && plain_folder(&full(root, &rel)) {
                            let _ = scan(root, &rel, &mut |child, _, _| shared.push(CREATED, child), &mut |_| Ok(true));
                        }
                    }
                    REMOVED | OLD_NAME => shared.push(DELETED, &rel),
                    MODIFIED if attributes & ATTRIBUTE_DIRECTORY == 0 => shared.push(CHANGED, &rel),
                    _ => {}
                }
                if next == 0 {
                    break;
                }
                at += next;
            }
            // A folder moved away keeps its handle, and its changes would come under the old
            // name: it is gone for the watcher.
            if final_path(handle) != first || std::fs::metadata(full(root, b"")).map(|info| !info.is_dir()).unwrap_or(true) {
                shared.end();
                return;
            }
        }
    }
}
