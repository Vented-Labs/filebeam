//! No-follow filesystem enumeration for private manifest trees.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use filebeam_transfer::manifest::{ManifestTree, ManifestTreeEntry};

use super::{SourceSpec, UploadSource};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TreeEnumerationOptions {
    pub skip_unsupported: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumeratedSourceFile {
    pub path: Vec<String>,
    pub source: UploadSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedSourceEntry {
    pub source_path: PathBuf,
    pub tree_path: Vec<String>,
    pub kind: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnumeratedSourceTree {
    pub roots: Vec<String>,
    pub files: Vec<EnumeratedSourceFile>,
    /// Includes non-root directories so empty directories remain representable.
    pub directories: Vec<Vec<String>>,
    pub skipped: Vec<SkippedSourceEntry>,
}

impl EnumeratedSourceTree {
    /// Associate source files with the transfer item IDs allocated by the
    /// existing preparation engine.
    pub fn manifest_tree(&self, item_ids: &[String]) -> Result<ManifestTree> {
        if item_ids.len() != self.files.len() {
            bail!("source file count does not match allocated manifest item IDs");
        }
        let mut entries = Vec::with_capacity(self.directories.len() + self.files.len());
        entries.extend(
            self.directories
                .iter()
                .cloned()
                .map(|path| ManifestTreeEntry {
                    path,
                    item_id: None,
                }),
        );
        entries.extend(
            self.files
                .iter()
                .zip(item_ids)
                .map(|(file, item_id)| ManifestTreeEntry {
                    path: file.path.clone(),
                    item_id: Some(item_id.clone()),
                }),
        );
        Ok(ManifestTree {
            roots: self.roots.clone(),
            entries,
        })
    }
}

/// Enumerate selected filesystem roots without following symlinks. Hidden
/// entries are ordinary names and are intentionally retained.
pub fn enumerate_source_tree(
    selected: &[PathBuf],
    options: TreeEnumerationOptions,
) -> Result<EnumeratedSourceTree> {
    let mut tree = EnumeratedSourceTree::default();
    let mut roots = HashSet::new();
    for source_path in selected {
        let root = path_name(source_path)?;
        if !roots.insert(root.clone()) {
            bail!("selected source roots have the same name: {root}");
        }
        let metadata = fs::symlink_metadata(source_path)
            .with_context(|| format!("inspect source {}", source_path.display()))?;
        let path = vec![root.clone()];
        if metadata.file_type().is_file() {
            tree.roots.push(root.clone());
            tree.files
                .push(source_file(source_path.clone(), path, root, metadata.len()));
        } else if metadata.file_type().is_dir() {
            tree.roots.push(root);
            enumerate_directory(source_path, &path, &mut tree, options)?;
        } else {
            unsupported(
                source_path,
                path,
                entry_kind(&metadata.file_type()),
                &mut tree,
                options,
            )?;
        }
    }
    Ok(tree)
}

fn enumerate_directory(
    directory: &Path,
    parent: &[String],
    tree: &mut EnumeratedSourceTree,
    options: TreeEnumerationOptions,
) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(directory)
        .with_context(|| format!("read source directory {}", directory.display()))?
        .collect::<std::result::Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let source_path = entry.path();
        let name = entry_name(&entry)?;
        let mut path = parent.to_vec();
        path.push(name.clone());
        let metadata = fs::symlink_metadata(&source_path)
            .with_context(|| format!("inspect source {}", source_path.display()))?;
        if metadata.file_type().is_file() {
            tree.files
                .push(source_file(source_path, path, name, metadata.len()));
        } else if metadata.file_type().is_dir() {
            tree.directories.push(path.clone());
            enumerate_directory(&source_path, &path, tree, options)?;
        } else {
            unsupported(
                &source_path,
                path,
                entry_kind(&metadata.file_type()),
                tree,
                options,
            )?;
        }
    }
    Ok(())
}

fn source_file(
    source_path: PathBuf,
    path: Vec<String>,
    name: String,
    length: u64,
) -> EnumeratedSourceFile {
    EnumeratedSourceFile {
        path,
        source: UploadSource {
            name,
            spec: SourceSpec::Path {
                path: source_path,
                offset: 0,
                length,
            },
        },
    }
}

fn unsupported(
    source_path: &Path,
    tree_path: Vec<String>,
    kind: &'static str,
    tree: &mut EnumeratedSourceTree,
    options: TreeEnumerationOptions,
) -> Result<()> {
    if !options.skip_unsupported {
        bail!("unsupported source entry {}: {kind}", source_path.display());
    }
    tree.skipped.push(SkippedSourceEntry {
        source_path: source_path.to_owned(),
        tree_path,
        kind,
    });
    Ok(())
}

fn path_name(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .context("selected source root has no valid UTF-8 name")
}

fn entry_name(entry: &fs::DirEntry) -> Result<String> {
    entry
        .file_name()
        .into_string()
        .map_err(|_| anyhow::anyhow!("source entry has no valid UTF-8 name"))
}

fn entry_kind(file_type: &fs::FileType) -> &'static str {
    if file_type.is_symlink() {
        "symlink"
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt;
            if file_type.is_fifo() {
                return "FIFO";
            }
            if file_type.is_socket() {
                return "socket";
            }
            if file_type.is_block_device() {
                return "block device";
            }
            if file_type.is_char_device() {
                return "character device";
            }
        }
        "special file"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_roots_hidden_files_and_empty_directories() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let root = temporary.path().join("photos");
        fs::create_dir_all(root.join("empty"))?;
        fs::write(root.join(".hidden"), b"hidden")?;
        fs::write(root.join("visible.txt"), b"visible")?;

        let tree = enumerate_source_tree(&[root], TreeEnumerationOptions::default())?;
        assert_eq!(tree.roots, ["photos"]);
        assert_eq!(
            tree.directories,
            vec![vec![String::from("photos"), String::from("empty")]]
        );
        assert_eq!(tree.files.len(), 2);
        assert_eq!(tree.files[0].path, vec!["photos", ".hidden"]);
        assert_eq!(tree.files[1].path, vec!["photos", "visible.txt"]);
        let manifest_tree = tree.manifest_tree(&["one".into(), "two".into()])?;
        assert_eq!(manifest_tree.entries.len(), 3);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_and_special_files_without_following_them() -> Result<()> {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir()?;
        let root = temporary.path().join("root");
        fs::create_dir(&root)?;
        let outside = temporary.path().join("outside.txt");
        fs::write(&outside, b"outside")?;
        symlink(&outside, root.join("linked"))?;
        let fifo = root.join("pipe");
        let status = std::process::Command::new("mkfifo").arg(&fifo).status()?;
        assert!(status.success());

        assert!(
            enumerate_source_tree(
                std::slice::from_ref(&root),
                TreeEnumerationOptions::default()
            )
            .is_err()
        );
        let tree = enumerate_source_tree(
            &[root],
            TreeEnumerationOptions {
                skip_unsupported: true,
            },
        )?;
        assert_eq!(tree.files.len(), 0);
        assert_eq!(tree.skipped.len(), 2);
        assert_eq!(tree.skipped[0].kind, "symlink");
        assert_eq!(tree.skipped[1].kind, "FIFO");
        Ok(())
    }

    #[test]
    fn uses_the_shared_tree_fixture() -> Result<()> {
        let manifest: filebeam_transfer::manifest::Manifest =
            serde_json::from_str(include_str!("../../../transfer/tests/fixtures/tree.json"))?;
        assert!(manifest.tree.is_some());
        Ok(())
    }
}
