//! Small filesystem helpers for artifact emission.

use std::{fs, path::Path};

pub fn write_text_file(path: &str, contents: &str) -> Result<(), String> {
    let path_ref = Path::new(path);
    if let Some(parent) = path_ref.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create `{}`: {err}", parent.display()))?;
    }
    fs::write(path_ref, contents).map_err(|err| format!("failed to write `{path}`: {err}"))
}
