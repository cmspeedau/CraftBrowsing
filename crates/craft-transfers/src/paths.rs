use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

const TRANSFER_DIR: &str = "CraftBrowsing";

/// Default download directory: `<OS Downloads>/CraftBrowsing`.
pub fn default_download_dir() -> PathBuf {
    let downloads = dirs::download_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Downloads")
    });
    downloads.join(TRANSFER_DIR)
}

/// Resolve a user-supplied directory, making relative paths absolute.
pub fn resolve_user_dir(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path.as_ref();
    if path.as_os_str().is_empty() {
        bail!("download folder path is empty");
    }
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .context("current directory is not set")?
            .join(path))
    }
}

/// Ensure a directory exists and is a directory.
pub fn ensure_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)
        .with_context(|| format!("failed to create download folder {}", path.display()))?;
    if !path.is_dir() {
        bail!("download folder is not a directory: {}", path.display());
    }
    Ok(())
}

/// Resolve, create, and canonicalize a user-supplied download directory.
pub fn persistable_dir(path: impl AsRef<Path>) -> Result<PathBuf> {
    let resolved = resolve_user_dir(path)?;
    ensure_dir(&resolved)?;
    Ok(std::fs::canonicalize(&resolved).unwrap_or(resolved))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dir_ends_with_craftbrowsing() {
        assert_eq!(
            default_download_dir()
                .file_name()
                .and_then(|name| name.to_str()),
            Some("CraftBrowsing")
        );
    }

    #[test]
    fn relative_paths_become_absolute() {
        let resolved = resolve_user_dir("nested/out").unwrap();
        assert!(resolved.is_absolute());
        assert!(resolved.ends_with(std::path::Path::new("nested").join("out")));
    }

    #[test]
    fn ensure_dir_creates_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("transfers");
        ensure_dir(&dest).unwrap();
        assert!(dest.is_dir());
        let stored = persistable_dir(&dest).unwrap();
        assert!(stored.is_absolute());
        assert!(stored.is_dir());
    }
}
