use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{Entry, EntryState, Issue, Report, directory_entry};

/// Only the two visible levels need rebuilding as a branch finishes. Existing
/// and newly completed subtrees are shared between snapshots.
pub(super) struct Preview {
    root: Arc<Entry>,
    pending: HashSet<PathBuf>,
    changes: HashMap<PathBuf, HashMap<PathBuf, Arc<Entry>>>,
    published: Instant,
}

impl Preview {
    pub(super) fn new(root: Arc<Entry>) -> Self {
        let pending = root
            .children
            .iter()
            .flat_map(|child| std::iter::once(child).chain(child.children.iter()))
            .filter(|entry| entry.state == EntryState::Pending)
            .map(|entry| entry.path.clone())
            .collect();
        Self {
            root,
            pending,
            changes: HashMap::new(),
            published: Instant::now(),
        }
    }

    pub(super) fn record(&mut self, parent: &Path, entry: &Arc<Entry>) {
        if self.pending.remove(&entry.path) {
            self.changes
                .entry(parent.to_path_buf())
                .or_default()
                .insert(entry.path.clone(), Arc::clone(entry));
        }
    }

    pub(super) fn publish(&mut self, issues: &[Issue], excluded_count: u64) -> Option<Report> {
        if self.changes.is_empty() || self.published.elapsed() < Duration::from_millis(400) {
            return None;
        }
        let direct = self.changes.get(&self.root.path);
        let children = self
            .root
            .children
            .iter()
            .map(|child| {
                if let Some(finished) = direct.and_then(|map| map.get(&child.path)) {
                    return Arc::clone(finished);
                }
                if let Some(nested) = self.changes.get(&child.path) {
                    let children = child
                        .children
                        .iter()
                        .map(|entry| Arc::clone(nested.get(&entry.path).unwrap_or(entry)))
                        .collect();
                    rebuild(child, children)
                } else {
                    Arc::clone(child)
                }
            })
            .collect();
        self.root = rebuild(&self.root, children);
        self.changes.clear();
        self.published = Instant::now();
        Some(Report {
            root: Arc::clone(&self.root),
            issues: issues.to_vec(),
            excluded_count,
            is_preview: true,
        })
    }
}

fn rebuild(previous: &Entry, children: Vec<Arc<Entry>>) -> Arc<Entry> {
    let state = match previous.state {
        EntryState::Protected | EntryState::Incomplete => previous.state,
        _ => EntryState::Local,
    };
    directory_entry(previous.path.clone(), state, children).0
}
