///! File system utilities, particularly functionality that requires
///! alternative implementations for alternative operating systems.
use std::{
    fs::{self, File},
    io, os,
    path::Path,
};

use common::eyre::{eyre, Result};

/// Set permissions on a file
#[allow(unused_variables)]
pub fn set_perms<File: AsRef<Path>>(path: File, mode: u32) -> Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }

    Ok(())
}

/// Create a symbolic (soft) link to a file
pub fn symlink_file<Original: AsRef<Path>, Link: AsRef<Path>>(
    original: Original,
    link: Link,
) -> Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    os::unix::fs::symlink(original, link)?;

    #[cfg(target_os = "windows")]
    os::windows::fs::symlink_file(original, link)?;

    Ok(())
}

/// Create a symbolic (soft) link to a directory
pub fn symlink_dir<Original: AsRef<Path>, Link: AsRef<Path>>(
    original: Original,
    link: Link,
) -> Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    os::unix::fs::symlink(original, link)?;

    #[cfg(target_os = "windows")]
    os::windows::fs::symlink_dir(original, link)?;

    Ok(())
}

/// Remove a file or directory if it exists
pub fn remove_if_exists(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    if path.exists() {
        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

/// Copy a file or directory if it exists
pub fn copy_if_exists(src: impl AsRef<Path>, dest: impl AsRef<Path>) -> Result<()> {
    let src = src.as_ref();
    if src.exists() {
        if src.is_dir() {
            copy_dir_all(src, dest)?;
        } else {
            fs::copy(src, dest)?;
        }
    }
    Ok(())
}

/// Clear a directory
pub fn clear_dir_all(dir: impl AsRef<Path>) -> Result<()> {
    fs::remove_dir_all(&dir)?;
    fs::create_dir_all(&dir)?;
    Ok(())
}

/// Recursively copy a directory to another
pub fn copy_dir_all(src: impl AsRef<Path>, dest: impl AsRef<Path>) -> Result<()> {
    fs::create_dir_all(&dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            copy_dir_all(entry.path(), dest.as_ref().join(entry.file_name()))?;
        } else if let Err(error) = fs::copy(entry.path(), dest.as_ref().join(entry.file_name())) {
            // Ignore "the source path is neither a regular file nor a symlink to a regular file" errors
            if !matches!(error.kind(), io::ErrorKind::InvalidInput) {
                return Err(eyre!(error));
            }
        }
    }
    Ok(())
}

/// Move a directory
///
/// This is a lot less efficient than `std::fs::rename` but will work across mounts
pub fn move_dir_all(src: impl AsRef<Path>, dest: impl AsRef<Path>) -> Result<()> {
    copy_dir_all(&src, &dest)?;
    fs::remove_dir_all(&src)?;
    Ok(())
}

/// Open a file in 600 mode (only read and writeable by current user)
pub fn open_file_600(path: impl AsRef<Path>) -> Result<File> {
    let mut options = fs::OpenOptions::new();
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(any(target_os = "windows"))]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }

    let file = options
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;

    Ok(file)
}
