use std::fs::File;
use std::path::Path;
use std::time::SystemTime;

pub struct FileWatcher {
    f: File,
    modified: SystemTime,
}

impl FileWatcher {
    pub fn new(file: impl AsRef<Path>) -> anyhow::Result<Self> {
        let f = File::open(file)?;
        let modified = f.metadata()?.modified()?;
        Ok(FileWatcher { f, modified })
    }

    pub fn changed(&mut self) -> anyhow::Result<bool> {
        let new_mod = self.f.metadata()?.modified()?;
        if self.modified != new_mod {
            self.modified = new_mod;
            return Ok(true);
        } else {
            return Ok(false);
        }
    }
}
