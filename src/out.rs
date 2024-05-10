use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub struct Out {
    prefix: PathBuf,
}

impl Out {
    /// Create a new out writer at `path`.
    ///
    /// # Warning
    ///
    /// This recursively removes everything currently at `path`.
    pub fn at(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();

        let _ = std::fs::remove_dir_all(path);
        fs::create_dir_all(path)?;

        Ok(Out {
            prefix: path.canonicalize()?,
        })
    }

    /// Copy a file by copying all bytes from `in_file` to `out_file`. This does not copy file
    /// attributes. Recursively creates `out_path` if it or its directory does not yet exist.
    pub fn copy_file(&self, in_file: impl AsRef<Path>, out_file: impl AsRef<Path>) -> anyhow::Result<()> {
        let mut fr = File::open(in_file)?;
        self.update_file(&mut fr, out_file)?;

        Ok(())
    }

    /// Write a file with the given `content` to `out_file`. Recursively creates `out_path` if it or
    /// its directory does not yet exist.
    pub fn update_file(&self, content: &mut impl Read, out_file: impl AsRef<Path>) -> anyhow::Result<()> {
        let out_file = self.prefix.join(out_file);

        if let Some(parent) = out_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut fw = File::create(out_file)?;
        io::copy(content, &mut fw)?;

        Ok(())
    }

    /// Concatenate all files in `in_dir` to `out_file`. Recursively creates `out_path` if it or
    /// its directory does not yet exist. `out_file` is only created if there are files in
    /// `in_dir`.
    pub fn cat_dir(&self, in_dir: impl AsRef<Path>, out_file: impl AsRef<Path>) -> anyhow::Result<()> {
        let out_file = self.prefix.join(out_file);

        if let Some(parent) = out_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut fw = None;

        for entry in walkdir::WalkDir::new(in_dir)
            .follow_links(true)
            .sort_by_file_name()
            .max_depth(1)
        {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }

            // Create the file handle only if there are actually files inside this directory to
            // concatenate.
            if fw.is_none() {
                fw = Some(File::create(&out_file)?);
            }

            let fw = fw.as_mut().unwrap();

            let path = entry.path();
            let mut fr = File::open(path)?;
            io::copy(&mut fr, fw)?;
        }

        Ok(())
    }

    /// Copy all files and directories from `in_dir` to `out_dir`. Files are copied by copying bytes.
    /// This does not copy file/directory attributes.
    pub fn copy_dir(&self, in_dir: impl AsRef<Path>, out_dir: impl AsRef<Path>) -> anyhow::Result<()> {
        let in_dir = in_dir.as_ref();
        let out_dir = out_dir.as_ref();

        for entry in walkdir::WalkDir::new(in_dir).follow_links(true) {
            let entry = entry?;
            let target = out_dir.join(entry.path().strip_prefix(in_dir)?);
            if entry.file_type().is_dir() {
                let target = self.prefix.join(target);
                fs::create_dir_all(target)?;
            } else if entry.file_type().is_file() {
                self.copy_file(entry.path(), target)?;
            }
        }

        Ok(())
    }

    pub fn write_file(&self, out_file: impl AsRef<Path>) -> anyhow::Result<std::fs::File> {
        let out_file = self.prefix.join(out_file);

        if let Some(parent) = out_file.parent() {
            fs::create_dir_all(parent)?;
        }

        Ok(File::create(out_file)?)
    }
}
