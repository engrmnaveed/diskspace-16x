//! Pro's per-entry details: each entry's modification time and owner.
//!
//! The tree keeps them in a side table beside its nodes, indexed by `NodeId`, so the
//! nodes themselves stay the same size. In a Basic build `Details` has no fields, which
//! makes the side table and every copy of it take no memory and compile to nothing.

/// A file's modification time, or a folder's newest one below it, and the owner.
#[cfg(feature = "pro")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Details {
    /// Whole seconds since 1970, clamped to 1 through `u32::MAX`. Zero means unknown, as
    /// for a folder without files or an imported scan.
    modified: u32,
    owner: u32,
}

/// Basic builds read and keep no details. The private field keeps it zero-sized.
#[cfg(not(feature = "pro"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Details(());

#[cfg(feature = "pro")]
impl Details {
    pub fn new(modified: i64, owner: u32) -> Self {
        Self {
            modified: modified.clamp(1, i64::from(u32::MAX)) as u32,
            owner,
        }
    }

    pub fn from_stat(metadata: &libc::stat) -> Self {
        Self::new(metadata.st_mtime, metadata.st_uid)
    }

    /// The modification time in seconds since 1970. For a folder, the newest time of
    /// any file below it. None when unknown.
    pub fn modified(self) -> Option<u32> {
        (self.modified != 0).then_some(self.modified)
    }

    #[allow(dead_code, reason = "scan --by-owner reads owners")]
    pub fn owner(self) -> u32 {
        self.owner
    }

    /// A folder's details before its contents are added: its owner, and no time yet.
    pub(super) fn folder(self) -> Self {
        Self {
            modified: 0,
            owner: self.owner,
        }
    }

    /// Adds a child's time to its folder's, keeping the newer one.
    pub(super) fn add(self, child: Self) -> Self {
        Self {
            modified: self.modified.max(child.modified),
            owner: self.owner,
        }
    }
}

#[cfg(not(feature = "pro"))]
impl Details {
    pub fn from_stat(_metadata: &libc::stat) -> Self {
        Self(())
    }

    pub fn modified(self) -> Option<u32> {
        None
    }

    pub(super) fn folder(self) -> Self {
        self
    }

    pub(super) fn add(self, _child: Self) -> Self {
        self
    }
}
