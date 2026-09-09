use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use tempfile::TempDir;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::{
    app::{Control, Phase, PromptKind},
    protocol::safe_filename,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DirectoryMode {
    Ask,
    RequireFlag,
    Zip,
    Individual,
}

pub struct UploadFile {
    pub path: PathBuf,
    pub name: String,
}

pub struct Prepared {
    pub files: Vec<UploadFile>,
    // Retain the temporary archive until the transfer worker is finished.
    _archive: Option<TempDir>,
}

struct Source {
    path: PathBuf,
    name: String,
    directory: bool,
    size: u64,
}

pub fn prepare(
    paths: &[PathBuf],
    mut mode: DirectoryMode,
    maximum_files: Option<usize>,
    control: &Control,
) -> Result<Prepared> {
    control.phase(Phase::Preparing)?;
    let mut sources = Vec::new();
    let mut roots = HashSet::new();
    let mut root_names = HashSet::new();
    let mut has_directory = false;
    for path in paths {
        control.check()?;
        let path = path
            .canonicalize()
            .with_context(|| format!("read {}", path.display()))?;
        if !roots.insert(path.clone()) {
            continue;
        }
        let metadata = path.metadata()?;
        if !metadata.is_file() && !metadata.is_dir() {
            bail!("{} is not a regular file or directory", path.display());
        }
        has_directory |= metadata.is_dir();
        let name = unique_name(
            &safe_filename(&path.file_name().unwrap_or_default().to_string_lossy()),
            &mut root_names,
        );
        collect(&path, &name, &mut sources, control)?;
    }
    let mut unique_paths = HashSet::new();
    let files = sources
        .iter()
        .filter(|source| !source.directory && unique_paths.insert(source.path.clone()))
        .count();
    let total = sources
        .iter()
        .try_fold(0_u64, |sum, source| sum.checked_add(source.size))
        .context("selected files are too large")?;
    if has_directory && matches!(mode, DirectoryMode::Ask | DirectoryMode::RequireFlag) {
        if mode == DirectoryMode::RequireFlag {
            bail!(
                "Directory uploads require --zip or --individual in plain or non-interactive mode"
            );
        }
        let answer = control.ask(PromptKind::Directory {
            files,
            bytes: total,
            maximum_files,
        })?;
        mode = match answer.as_str() {
            "zip" => DirectoryMode::Zip,
            "individual" => DirectoryMode::Individual,
            _ => bail!("Choose ZIP and send or Individual files"),
        };
    }
    if mode == DirectoryMode::Zip {
        control.phase(Phase::Archiving)?;
        let directory = tempfile::tempdir().context("create temporary ZIP directory")?;
        let name = if paths.len() == 1 {
            let path = paths[0].canonicalize()?;
            format!(
                "{}.zip",
                safe_filename(&path.file_name().unwrap_or_default().to_string_lossy())
            )
        } else {
            "filebeam-transfer.zip".into()
        };
        control.item(name.clone(), 1, total);
        let path = directory.path().join(&name);
        let mut archive = ZipWriter::new(File::create(&path)?);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(1))
            .unix_permissions(0o644);
        let mut buffer = vec![0; 64 * 1024];
        for source in &sources {
            control.check()?;
            if source.directory {
                archive.add_directory(&source.name, options.unix_permissions(0o755))?;
            } else {
                archive.start_file(
                    &source.name,
                    options.large_file(source.size >= u32::MAX as u64),
                )?;
                let mut file = File::open(&source.path)
                    .with_context(|| format!("read {}", source.path.display()))?;
                loop {
                    control.check()?;
                    let count = file.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    archive.write_all(&buffer[..count])?;
                }
            }
        }
        archive.finish()?.sync_all()?;
        control.phase(Phase::Preparing)?;
        return Ok(Prepared {
            files: vec![UploadFile { path, name }],
            _archive: Some(directory),
        });
    }
    if files == 0 {
        bail!("No regular files found; use --zip to send an empty directory");
    }
    if let Some(maximum) = maximum_files
        && files > maximum
    {
        bail!(
            "{files} individual files exceed this instance's {maximum}-file limit; use --zip to send one archive"
        );
    }
    let mut names = HashSet::new();
    let mut seen = HashSet::new();
    let files = sources
        .into_iter()
        .filter(|source| !source.directory && seen.insert(source.path.clone()))
        .map(|source| {
            let name = safe_filename(
                &source
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy(),
            );
            UploadFile {
                path: source.path,
                name: unique_name(&name, &mut names),
            }
        })
        .collect();
    Ok(Prepared {
        files,
        _archive: None,
    })
}

fn collect(path: &Path, name: &str, entries: &mut Vec<Source>, control: &Control) -> Result<()> {
    // An iterative traversal avoids stack growth on deeply nested folders.
    let mut pending = vec![(path.to_owned(), name.to_owned())];
    let mut archive_names = HashSet::new();
    while let Some((path, name)) = pending.pop() {
        control.check()?;
        let metadata =
            fs::symlink_metadata(&path).with_context(|| format!("read {}", path.display()))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let directory = metadata.is_dir();
        if !directory && !metadata.is_file() {
            continue;
        }
        if !archive_names.insert(name.clone()) {
            bail!("Two entries have the same portable ZIP path: {name}");
        }
        entries.push(Source {
            path: path.clone(),
            name: name.clone(),
            directory,
            size: if directory { 0 } else { metadata.len() },
        });
        if directory {
            let mut children = fs::read_dir(&path)
                .with_context(|| format!("open directory {}", path.display()))?
                .collect::<std::io::Result<Vec<_>>>()?;
            children.sort_by_key(|entry| entry.file_name());
            for child in children.into_iter().rev() {
                pending.push((
                    child.path(),
                    format!(
                        "{name}/{}",
                        safe_filename(&child.file_name().to_string_lossy())
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn unique_name(name: &str, used: &mut HashSet<String>) -> String {
    if used.insert(name.to_owned()) {
        return name.to_owned();
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    for index in 2.. {
        let candidate = format!("{stem} ({index}){extension}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!()
}
