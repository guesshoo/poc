use std::{
    fs::{self, File},
    io,
    path::Path,
};

/// Ensure that `path` exists on disk.
/// 
/// - If `path` has no file-extension, it’s treated as a directory:
///   it will create that dir and all parents.
/// - If it has an extension (e.g. “wal.log”), it will:
///     1. create the parent directory chain
///     2. create the file if it doesn’t exist (empty).
pub fn ensure_path_exists<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let path = path.as_ref();

    // If it already exists, nothing to do.
    if path.exists() {
        return Ok(());
    }

    if path.extension().is_some() {
        // Looks like a file: create parent dirs, then touch the file.
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        // Create the file (fails if already exists, so .exists() check above avoids that).
        File::create(path)?;
    } else {
        // No extension → treat as directory.
        fs::create_dir_all(path)?;
    }

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::ensure_path_exists;
    use std::{fs, path::Path};

    #[test]
    fn create_dir() {
        let dir = Path::new("/tmp/cache/meta");
        // Clean up before test
        let _ = fs::remove_dir_all(&dir);
        ensure_path_exists(&dir).unwrap();
        assert!(dir.exists() && dir.is_dir());
    }

    #[test]
    fn create_file_and_parent_dirs() {
        let file = Path::new("/tmp/cache/data/wal.log");
        // Clean up before test
        let _ = fs::remove_file(&file);
        let _ = fs::remove_dir_all(file.parent().unwrap());
        ensure_path_exists(&file).unwrap();
        assert!(file.exists() && file.is_file());
    }
}
