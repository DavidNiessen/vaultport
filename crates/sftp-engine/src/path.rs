// SPDX-License-Identifier: GPL-3.0-or-later
//! Path handling that never lets a (possibly malicious) entry name escape its directory.
//!
//! A server controls the names it returns from `READDIR`. A name such as `../../.bashrc`,
//! `/etc/passwd`, `..\\evil` or `C:evil` must never be joined into a local path, and names
//! containing `/` must never be joined into remote paths. Every join in this crate goes through
//! [`validate_name`].

use crate::error::FsError;
use std::path::{Component, Path, PathBuf};

/// Checks that `name` is a single, plain path component on every platform.
pub fn validate_name(name: &str) -> Result<(), FsError> {
    let bad = name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
        || name.chars().any(char::is_control);
    if bad {
        return Err(FsError::UnsafePath(name.to_owned()));
    }
    Ok(())
}

/// Additional restrictions for names written to the local file system.
fn validate_local_name(name: &str) -> Result<(), FsError> {
    validate_name(name)?;
    if cfg!(windows) {
        let stem = name
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || ((stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.len() == 4
                && stem.as_bytes()[3].is_ascii_digit());
        if reserved
            || name.contains(':')
            || name.ends_with('.')
            || name.ends_with(' ')
            || name
                .chars()
                .any(|c| matches!(c, '<' | '>' | '"' | '|' | '?' | '*'))
        {
            return Err(FsError::UnsafePath(name.to_owned()));
        }
    }
    // Belt and braces: the platform must see exactly one normal component.
    let mut comps = Path::new(name).components();
    match (comps.next(), comps.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(FsError::UnsafePath(name.to_owned())),
    }
}

/// Joins an untrusted entry `name` to a local directory.
pub fn safe_local_join(base: &Path, name: &str) -> Result<PathBuf, FsError> {
    validate_local_name(name)?;
    Ok(base.join(name))
}

/// Joins an untrusted entry `name` to a remote (POSIX) directory.
pub fn remote_join(dir: &str, name: &str) -> Result<String, FsError> {
    validate_name(name)?;
    Ok(if dir.is_empty() {
        name.to_owned()
    } else if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    })
}

/// Parent of a remote path (`/a/b` → `/a`, `/a` → `/`, `a` → `None`).
#[must_use]
pub fn remote_parent(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.rfind('/') {
        Some(0) => Some("/".to_owned()),
        Some(i) => Some(trimmed[..i].to_owned()),
        None => None,
    }
}

/// Last component of a remote path.
#[must_use]
pub fn remote_file_name(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    let name = trimmed.rsplit('/').next()?;
    (!name.is_empty() && name != "." && name != "..").then(|| name.to_owned())
}

/// `name (1).ext`, `name (2).ext`, … for "keep both" conflict resolution.
#[must_use]
pub fn numbered_name(name: &str, n: u32) -> String {
    // A leading dot is part of the stem (".bashrc" → ".bashrc (1)").
    let split = name.rfind('.').filter(|&i| i > 0);
    match split {
        Some(i) => format!("{} ({n}){}", &name[..i], &name[i..]),
        None => format!("{name} ({n})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_names_are_rejected() {
        let base = Path::new("/tmp/download");
        for evil in [
            "..",
            ".",
            "",
            "../../.bashrc",
            "a/../../b",
            "/etc/passwd",
            "..\\..\\evil",
            "dir\\file",
            "nul\0byte",
            "line\nbreak",
        ] {
            assert!(safe_local_join(base, evil).is_err(), "{evil:?} accepted");
            assert!(
                remote_join("/srv", evil).is_err(),
                "{evil:?} accepted remotely"
            );
        }
        assert_eq!(
            safe_local_join(base, "..hidden").expect("ok"),
            base.join("..hidden")
        );
        assert_eq!(
            safe_local_join(base, "a b.txt").expect("ok"),
            base.join("a b.txt")
        );
    }

    #[test]
    fn remote_paths() {
        assert_eq!(remote_join("/", "a").expect("ok"), "/a");
        assert_eq!(remote_join("/a", "b").expect("ok"), "/a/b");
        assert_eq!(remote_join("", "b").expect("ok"), "b");
        assert_eq!(remote_parent("/a/b/").as_deref(), Some("/a"));
        assert_eq!(remote_parent("/a").as_deref(), Some("/"));
        assert_eq!(remote_parent("/"), None);
        assert_eq!(remote_file_name("/a/b.txt").as_deref(), Some("b.txt"));
        assert_eq!(remote_file_name("/"), None);
    }

    #[test]
    fn keep_both_names() {
        assert_eq!(numbered_name("file.txt", 1), "file (1).txt");
        assert_eq!(numbered_name("archive.tar.gz", 2), "archive.tar (2).gz");
        assert_eq!(numbered_name(".bashrc", 1), ".bashrc (1)");
        assert_eq!(numbered_name("dir", 3), "dir (3)");
    }
}
