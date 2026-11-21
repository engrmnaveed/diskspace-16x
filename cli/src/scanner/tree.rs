//! A scan result stored as an arena of name-only nodes.
//!
//! Nodes are laid out in depth-first order with each folder's children in name order, so
//! a folder's descendants immediately follow it. Each node keeps its parent's index and
//! the size of its subtree, and paths are rebuilt from the names on demand. Node indices
//! are stable for one tree, which lets later features keep per-node data in side tables.

use std::cmp::Ordering;
use std::ffi::OsStr;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use super::{EntryKind, EntryState};

/// A node's position in its tree.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(u32);

impl NodeId {
    pub const ROOT: Self = Self(0);

    fn index(self) -> usize {
        self.0 as usize
    }
}

const NO_PARENT: u32 = u32::MAX;
/// Indices and name offsets are 32-bit, which keeps each node at 40 bytes.
const LIMIT: usize = u32::MAX as usize - 1;

#[derive(Clone, Debug)]
struct Node {
    allocated: u64,
    apparent: u64,
    name_start: u32,
    name_len: u32,
    parent: u32,
    /// This node and its descendants.
    size: u32,
    /// Files and folders below this node, counting each hard-linked file once.
    items: u32,
    kind: EntryKind,
    state: EntryState,
    /// A folder's state before its contents are added: problems reading the folder
    /// itself or children that have no node. A file's own state is its state.
    own_state: EntryState,
}

#[derive(Clone, Debug)]
pub struct Tree {
    root_path: PathBuf,
    nodes: Vec<Node>,
    names: Vec<u8>,
}

impl Tree {
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub fn root(&self) -> Entry<'_> {
        self.entry(NodeId::ROOT)
    }

    pub fn entry(&self, id: NodeId) -> Entry<'_> {
        debug_assert!(id.index() < self.nodes.len());
        Entry { tree: self, id }
    }

    fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    fn name_bytes(&self, id: NodeId) -> &[u8] {
        let node = self.node(id);
        let start = node.name_start as usize;
        &self.names[start..start + node.name_len as usize]
    }

    fn parent_of(&self, id: NodeId) -> Option<NodeId> {
        let parent = self.node(id).parent;
        (parent != NO_PARENT).then_some(NodeId(parent))
    }

    fn depth(&self, mut id: NodeId) -> usize {
        let mut depth = 0;
        while let Some(parent) = self.parent_of(id) {
            id = parent;
            depth += 1;
        }
        depth
    }

    /// The full path of `id`, rebuilt from the names above it.
    pub fn path(&self, id: NodeId) -> PathBuf {
        let mut path = self.root_path.clone();
        // Pushing the root's empty relative path would add a trailing separator.
        if id != NodeId::ROOT {
            path.push(self.relative_path(id, NodeId::ROOT));
        }
        path
    }

    /// The path of `id` below `ancestor`, or the full path when `ancestor` isn't above it.
    pub fn relative_path(&self, id: NodeId, ancestor: NodeId) -> PathBuf {
        let mut names = Vec::new();
        let mut current = id;
        while current != ancestor {
            let Some(parent) = self.parent_of(current) else {
                return self.path(id);
            };
            names.push(self.name_bytes(current));
            current = parent;
        }
        let mut path = PathBuf::with_capacity(names.iter().map(|name| name.len() + 1).sum());
        for name in names.into_iter().rev() {
            path.push(OsStr::from_bytes(name));
        }
        path
    }

    /// Finds the node at `path`, which must be inside the root path.
    pub fn find(&self, path: &Path) -> Option<NodeId> {
        let relative = path.strip_prefix(&self.root_path).ok()?;
        let mut current = self.root();
        for name in relative.iter() {
            current = current.children().find(|child| child.name() == name)?;
        }
        Some(current.id)
    }

    /// Compares the full paths of two nodes byte by byte without building them.
    pub fn cmp_paths(&self, left: NodeId, right: NodeId) -> Ordering {
        if left == right {
            return Ordering::Equal;
        }
        let (mut a, mut b) = (left, right);
        if self.parent_of(a) == self.parent_of(b) {
            return compare_names(self.name_bytes(a), false, self.name_bytes(b), false)
                .then(left.cmp(&right));
        }
        let (depth_a, depth_b) = (self.depth(a), self.depth(b));
        // Whether more of the original path follows the current node.
        let (mut below_a, mut below_b) = (false, false);
        for _ in depth_b..depth_a {
            a = self.parent_of(a).expect("a deeper node has a parent");
            below_a = true;
        }
        for _ in depth_a..depth_b {
            b = self.parent_of(b).expect("a deeper node has a parent");
            below_b = true;
        }
        if a == b {
            // One path contains the other, and the shorter one sorts first.
            return if below_a {
                Ordering::Greater
            } else {
                Ordering::Less
            };
        }
        while self.parent_of(a) != self.parent_of(b) {
            a = self
                .parent_of(a)
                .expect("nodes below the root have parents");
            b = self
                .parent_of(b)
                .expect("nodes below the root have parents");
            below_a = true;
            below_b = true;
        }
        compare_names(self.name_bytes(a), below_a, self.name_bytes(b), below_b)
            .then(left.cmp(&right))
    }

    /// A copy without the subtrees at `removed`. Folder totals and states above them are
    /// recomputed. Hard-link aliases stay as they were.
    pub fn without(&self, removed: &[NodeId]) -> Tree {
        let mut removed = removed.to_vec();
        removed.sort_unstable();
        self.rebuild(|id| {
            if removed.binary_search(&id).is_ok() {
                Visit::Skip
            } else {
                Visit::Keep
            }
        })
    }

    /// A copy with the folder at `at` replaced by `subtree`, whose root keeps the
    /// folder's name. Folder totals and states above it are recomputed.
    pub fn replacing(&self, at: NodeId, subtree: &Tree) -> Tree {
        self.rebuild(|id| {
            if id == at {
                Visit::Replace(subtree)
            } else {
                Visit::Keep
            }
        })
    }

    fn rebuild<'a>(&self, mut visit: impl FnMut(NodeId) -> Visit<'a>) -> Tree {
        let root = self.node(NodeId::ROOT);
        let mut builder = TreeBuilder::with_capacity(
            self.root_path.clone(),
            root.own_state,
            self.nodes.len(),
            self.names.len(),
        );
        builder.copy_below(self, NodeId::ROOT, &mut visit);
        builder.finish().0
    }
}

enum Visit<'a> {
    Keep,
    Skip,
    Replace(&'a Tree),
}

/// Compares two sibling names as parts of full paths. A name followed by more of its
/// path continues with a separator, and a name that ends its path sorts before any byte.
fn compare_names(left: &[u8], left_below: bool, right: &[u8], right_below: bool) -> Ordering {
    let common = left.len().min(right.len());
    match left[..common].cmp(&right[..common]) {
        Ordering::Equal => {}
        unequal => return unequal,
    }
    let next = |name: &[u8], below: bool| name.get(common).copied().or(below.then_some(b'/'));
    next(left, left_below).cmp(&next(right, right_below))
}

/// A node of a tree, with read access to its fields.
#[derive(Clone, Copy)]
pub struct Entry<'a> {
    tree: &'a Tree,
    id: NodeId,
}

impl<'a> Entry<'a> {
    pub fn id(self) -> NodeId {
        self.id
    }

    pub fn tree(self) -> &'a Tree {
        self.tree
    }

    fn node(self) -> &'a Node {
        self.tree.node(self.id)
    }

    pub fn allocated_bytes(self) -> u64 {
        self.node().allocated
    }

    /// File lengths (`st_size`), counted under the same rules as allocated bytes.
    pub fn apparent_bytes(self) -> u64 {
        self.node().apparent
    }

    /// Files and folders below this entry, counting each hard-linked file once.
    pub fn item_count(self) -> u64 {
        u64::from(self.node().items)
    }

    pub fn kind(self) -> EntryKind {
        self.node().kind
    }

    pub fn state(self) -> EntryState {
        self.node().state
    }

    /// A folder's own state, before its contents degrade it. A file's is its state.
    pub fn own_state(self) -> EntryState {
        self.node().own_state
    }

    /// The last path component. The root's is its path's final component.
    pub fn name(self) -> &'a OsStr {
        if self.id == NodeId::ROOT {
            let root = self.tree.root_path.as_path();
            return root.file_name().unwrap_or(root.as_os_str());
        }
        OsStr::from_bytes(self.tree.name_bytes(self.id))
    }

    pub fn path(self) -> PathBuf {
        self.tree.path(self.id)
    }

    pub fn children(self) -> Children<'a> {
        let first = self.id.0 + 1;
        Children {
            tree: self.tree,
            next: first,
            end: self.id.0 + self.node().size,
        }
    }

    /// Every node below this one, in depth-first order.
    pub fn descendants(self) -> impl Iterator<Item = Entry<'a>> {
        let tree = self.tree;
        (self.id.0 + 1..self.id.0 + self.node().size).map(move |index| tree.entry(NodeId(index)))
    }
}

impl std::fmt::Debug for Entry<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Entry")
            .field("path", &self.path())
            .field("allocated_bytes", &self.allocated_bytes())
            .field("state", &self.state())
            .finish()
    }
}

pub struct Children<'a> {
    tree: &'a Tree,
    next: u32,
    end: u32,
}

impl<'a> Iterator for Children<'a> {
    type Item = Entry<'a>;

    fn next(&mut self) -> Option<Entry<'a>> {
        if self.next >= self.end {
            return None;
        }
        let id = NodeId(self.next);
        self.next += self.tree.node(id).size;
        Some(self.tree.entry(id))
    }
}

/// Builds a tree in depth-first order. Folder totals, item counts and states are
/// computed from their children when each folder closes.
pub struct TreeBuilder {
    tree: Tree,
    open: Vec<u32>,
    overflowed: Vec<NodeId>,
    too_large: bool,
}

impl TreeBuilder {
    pub fn new(root_path: PathBuf, own_state: EntryState) -> Self {
        Self::with_capacity(root_path, own_state, 1, 0)
    }

    pub fn with_capacity(
        root_path: PathBuf,
        own_state: EntryState,
        nodes: usize,
        name_bytes: usize,
    ) -> Self {
        let mut tree = Tree {
            root_path,
            nodes: Vec::with_capacity(nodes.clamp(1, LIMIT)),
            names: Vec::with_capacity(name_bytes.min(LIMIT)),
        };
        tree.nodes.push(Node {
            allocated: 0,
            apparent: 0,
            name_start: 0,
            name_len: 0,
            parent: NO_PARENT,
            size: 1,
            items: 0,
            kind: EntryKind::Directory,
            state: own_state,
            own_state,
        });
        Self {
            tree,
            open: vec![0],
            overflowed: Vec::new(),
            too_large: false,
        }
    }

    fn push(&mut self, name: &[u8], mut node: Node) -> Option<NodeId> {
        let index = self.tree.nodes.len();
        let name_start = self.tree.names.len();
        if self.too_large || index >= LIMIT || name_start + name.len() >= LIMIT {
            self.too_large = true;
            return None;
        }
        node.parent = *self.open.last().expect("the root stays open until finish");
        node.name_start = name_start as u32;
        node.name_len = name.len() as u32;
        self.tree.names.extend_from_slice(name);
        self.tree.nodes.push(node);
        Some(NodeId(index as u32))
    }

    pub fn file(&mut self, name: &[u8], allocated: u64, apparent: u64, state: EntryState) {
        self.push(
            name,
            Node {
                allocated,
                apparent,
                name_start: 0,
                name_len: 0,
                parent: 0,
                size: 1,
                items: 0,
                kind: EntryKind::File,
                state,
                own_state: state,
            },
        );
    }

    /// Starts a folder. Its children follow until the matching `close_folder`.
    pub fn open_folder(&mut self, name: &[u8], own_state: EntryState) {
        if let Some(id) = self.push(
            name,
            Node {
                allocated: 0,
                apparent: 0,
                name_start: 0,
                name_len: 0,
                parent: 0,
                size: 1,
                items: 0,
                kind: EntryKind::Directory,
                state: own_state,
                own_state,
            },
        ) {
            self.open.push(id.0);
        } else {
            // Keep folder nesting balanced after reaching the size limit.
            self.open.push(u32::MAX);
        }
    }

    /// Adds an empty folder, such as a cloud-only or skipped one.
    pub fn folder(&mut self, name: &[u8], own_state: EntryState) {
        self.open_folder(name, own_state);
        self.close_folder();
    }

    pub fn close_folder(&mut self) {
        assert!(self.open.len() > 1, "only finish closes the root");
        let index = self.open.pop().expect("a folder is open");
        if index != u32::MAX {
            self.total(index as usize);
        }
    }

    /// Sums a folder's children, which all follow it and are complete.
    fn total(&mut self, index: usize) {
        let end = self.tree.nodes.len();
        let mut bytes = 0_u64;
        let mut apparent = 0_u64;
        let mut items = 0_u32;
        let mut overflow = false;
        let mut state = self.tree.nodes[index].own_state;
        let mut child = index + 1;
        while child < end {
            let node = &self.tree.nodes[child];
            for (total, amount) in [(&mut bytes, node.allocated), (&mut apparent, node.apparent)] {
                match total.checked_add(amount) {
                    Some(sum) => *total = sum,
                    None => {
                        *total = u64::MAX;
                        overflow = true;
                    }
                }
            }
            if node.state != EntryState::HardLinkAlias {
                items = items.saturating_add(node.items.saturating_add(1));
            }
            state = if node.state == EntryState::Pending {
                EntryState::Pending
            } else {
                state.degrade(node.state)
            };
            child += node.size as usize;
        }
        if overflow {
            state = state.degrade(EntryState::Incomplete);
            self.overflowed.push(NodeId(index as u32));
        }
        let node = &mut self.tree.nodes[index];
        node.allocated = bytes;
        node.apparent = apparent;
        node.items = items;
        node.state = state;
        node.size = (end - index) as u32;
    }

    /// Copies everything below `id` in `source`, consulting `visit` for each node.
    fn copy_below<'a>(
        &mut self,
        source: &Tree,
        id: NodeId,
        visit: &mut dyn FnMut(NodeId) -> Visit<'a>,
    ) {
        let start = id.0 + 1;
        let end = id.0 + source.node(id).size;
        let mut ends: Vec<u32> = Vec::new();
        let mut index = start;
        while index < end {
            while ends.last().is_some_and(|&folder_end| folder_end <= index) {
                ends.pop();
                self.close_folder();
            }
            let current = NodeId(index);
            let node = source.node(current);
            let name = source.name_bytes(current);
            match visit(current) {
                Visit::Skip => {}
                Visit::Replace(subtree) => {
                    let root = subtree.node(NodeId::ROOT);
                    self.open_folder(name, root.own_state);
                    subtree.copy_into(self);
                    self.close_folder();
                }
                Visit::Keep if node.kind == EntryKind::File => {
                    self.file(name, node.allocated, node.apparent, node.state);
                }
                Visit::Keep => {
                    self.open_folder(name, node.own_state);
                    ends.push(index + node.size);
                    index += 1;
                    continue;
                }
            }
            index += node.size;
        }
        for _ in ends {
            self.close_folder();
        }
    }

    /// Closes every open folder and returns the tree with the folders whose totals
    /// exceeded the byte range.
    pub fn finish(mut self) -> (Tree, Vec<NodeId>) {
        while self.open.len() > 1 {
            self.close_folder();
        }
        self.total(0);
        (self.tree, self.overflowed)
    }

    /// Like `finish`, but fails when the tree reached the arena's size limit.
    pub fn try_finish(self) -> io::Result<(Tree, Vec<NodeId>)> {
        if self.too_large {
            return Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "The scan has more entries than Space 16X supports.",
            ));
        }
        Ok(self.finish())
    }
}

impl Tree {
    /// Adds this tree's contents below the builder's open folder.
    fn copy_into(&self, builder: &mut TreeBuilder) {
        builder.copy_below(self, NodeId::ROOT, &mut |_| Visit::Keep);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Tree {
        let mut builder = TreeBuilder::new("/r".into(), EntryState::Local);
        builder.open_folder(b"a", EntryState::Local);
        builder.file(b"x", 10, 7, EntryState::Local);
        builder.open_folder(b"deep", EntryState::Local);
        builder.file(b"y", 5, 5, EntryState::Local);
        builder.close_folder();
        builder.close_folder();
        builder.file(b"a b", 1, 1, EntryState::Local);
        builder.file(b"alias", 0, 0, EntryState::HardLinkAlias);
        builder.folder(b"locked", EntryState::Protected);
        builder.finish().0
    }

    #[test]
    fn lays_out_nodes_depth_first_with_totals() {
        let tree = sample();
        let root = tree.root();
        assert_eq!(tree.nodes.len(), 8);
        assert_eq!(root.allocated_bytes(), 16);
        assert_eq!(root.apparent_bytes(), 13);
        // a, a/x, a/deep, a/deep/y, "a b", locked; the alias isn't an item.
        assert_eq!(root.item_count(), 6);
        assert_eq!(root.state(), EntryState::Protected);
        let names: Vec<_> = root
            .children()
            .map(|child| child.name().to_owned())
            .collect();
        assert_eq!(names, ["a", "a b", "alias", "locked"]);
        let deep = tree.find(Path::new("/r/a/deep")).unwrap();
        assert_eq!(tree.path(deep), Path::new("/r/a/deep"));
        assert_eq!(tree.entry(deep).item_count(), 1);
        assert_eq!(tree.entry(tree.parent_of(deep).unwrap()).name(), "a");
        assert_eq!(
            tree.relative_path(tree.find(Path::new("/r/a/deep/y")).unwrap(), NodeId::ROOT),
            Path::new("a/deep/y")
        );
        assert_eq!(root.name(), "r");
        assert_eq!(root.path(), Path::new("/r"));
        assert_eq!(root.path().as_os_str(), "/r");
        assert!(tree.find(Path::new("/r/missing")).is_none());
        assert!(tree.find(Path::new("/elsewhere")).is_none());
    }

    #[test]
    fn compares_paths_bytewise_without_building_them() {
        let tree = sample();
        let mut ids: Vec<_> = (0..tree.nodes.len() as u32).map(NodeId).collect();
        ids.sort_by(|&a, &b| tree.cmp_paths(a, b));
        let sorted: Vec<_> = ids.iter().map(|&id| tree.path(id)).collect();
        let mut expected = sorted.clone();
        expected.sort_by(|a, b| a.as_os_str().as_bytes().cmp(b.as_os_str().as_bytes()));
        assert_eq!(sorted, expected);
        // "/r/a b" sorts before "/r/a/x" bytewise, though a depth-first walk visits a first.
        let spaced = tree.find(Path::new("/r/a b")).unwrap();
        let nested = tree.find(Path::new("/r/a/x")).unwrap();
        assert_eq!(tree.cmp_paths(spaced, nested), Ordering::Less);
        assert!(nested < spaced);
    }

    #[test]
    fn removes_and_replaces_subtrees_with_recomputed_totals() {
        let tree = sample();
        let deep = tree.find(Path::new("/r/a/deep")).unwrap();
        let locked = tree.find(Path::new("/r/locked")).unwrap();
        let trimmed = tree.without(&[deep, locked]);
        assert_eq!(trimmed.nodes.len(), 5);
        assert_eq!(trimmed.root().allocated_bytes(), 11);
        assert_eq!(trimmed.root().item_count(), 3);
        assert_eq!(trimmed.root().state(), EntryState::Local);
        assert!(trimmed.find(Path::new("/r/a/deep/y")).is_none());

        let mut builder = TreeBuilder::new("/r/a/deep".into(), EntryState::Local);
        builder.file(b"new", 100, 90, EntryState::Local);
        builder.file(b"other", 1, 1, EntryState::Local);
        let replacement = builder.finish().0;
        let replaced = tree.replacing(deep, &replacement);
        let deep = replaced.find(Path::new("/r/a/deep")).unwrap();
        assert_eq!(replaced.entry(deep).allocated_bytes(), 101);
        assert_eq!(replaced.entry(deep).item_count(), 2);
        assert_eq!(replaced.root().allocated_bytes(), 112);
        assert!(replaced.find(Path::new("/r/a/deep/new")).is_some());
        assert!(replaced.find(Path::new("/r/a/deep/y")).is_none());
        assert_eq!(
            replaced.path(replaced.find(Path::new("/r/locked")).unwrap()),
            Path::new("/r/locked")
        );
    }

    #[test]
    fn reports_overflowing_folders_and_keeps_pending_states() {
        let mut builder = TreeBuilder::new("/".into(), EntryState::Local);
        builder.open_folder(b"big", EntryState::Local);
        builder.file(b"one", u64::MAX, 1, EntryState::Local);
        builder.file(b"two", 1, 1, EntryState::Local);
        builder.close_folder();
        builder.folder(b"later", EntryState::Pending);
        let (tree, overflowed) = builder.finish();
        let big = tree.find(Path::new("/big")).unwrap();
        assert_eq!(tree.path(big), Path::new("/big"));
        assert!(overflowed.contains(&big));
        assert_eq!(tree.entry(big).state(), EntryState::Incomplete);
        assert_eq!(tree.entry(big).allocated_bytes(), u64::MAX);
        assert_eq!(tree.root().state(), EntryState::Pending);
    }
}
