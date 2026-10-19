use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_MODULE_DEPTH: usize = 64;
const MAX_MODULE_SOURCE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_EXPANDED_SOURCE_BYTES: usize = 16 * 1024 * 1024;

fn read_module_source(path: &Path) -> Result<String, String> {
    let file = fs::File::open(path)
        .map_err(|error| format!("cannot read module {}: {error}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(MAX_MODULE_SOURCE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read module {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_MODULE_SOURCE_BYTES {
        return Err(format!(
            "module source exceeds safety limit ({MAX_MODULE_SOURCE_BYTES} bytes): {}",
            path.display()
        ));
    }
    String::from_utf8(bytes)
        .map_err(|error| format!("module source is not valid UTF-8 at {}: {error}", path.display()))
}

fn append_bounded(expanded: &mut String, text: &str) -> Result<(), String> {
    let next_len = expanded.len().checked_add(text.len())
        .ok_or_else(|| "expanded module source size overflow".to_string())?;
    if next_len > MAX_EXPANDED_SOURCE_BYTES {
        return Err(format!("expanded module source exceeds safety limit ({MAX_EXPANDED_SOURCE_BYTES} bytes)"));
    }
    expanded.push_str(text);
    Ok(())
}

/// Loads a .zv program and expands use "relative/path.zv" directives.
/// Paths are resolved relative to the file containing the directive.
pub fn load(path: &Path) -> Result<String, String> {
    let mut loaded = HashSet::new();
    let mut active = Vec::new();
    load_recursive(path, &mut loaded, &mut active)
}

fn load_recursive(
    path: &Path,
    loaded: &mut HashSet<PathBuf>,
    active: &mut Vec<PathBuf>,
) -> Result<String, String> {
    if active.len() >= MAX_MODULE_DEPTH {
        return Err(format!(
            "module import depth exceeds safety limit ({MAX_MODULE_DEPTH}) at {}",
            path.display()
        ));
    }

    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("cannot resolve module {}: {error}", path.display()))?;

    if active.contains(&canonical) {
        let chain = active
            .iter()
            .chain(std::iter::once(&canonical))
            .map(|item| item.display().to_string())
            .collect::<Vec<_>>()
            .join(" -> ");
        return Err(format!("cyclic module import: {chain}"));
    }

    if !loaded.insert(canonical.clone()) {
        return Ok(String::new());
    }

    let source = read_module_source(&canonical)?;
    active.push(canonical.clone());

    let mut expanded = String::new();
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("use ") {
            let module = parse_module_path(rest).map_err(|message| {
                format!("{}:{}: {message}", canonical.display(), line_index + 1)
            })?;
            let child_path = canonical
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(module);
            let child_source = load_recursive(&child_path, loaded, active).map_err(|error| {
                format!("{}:{}: {error}", canonical.display(), line_index + 1)
            })?;
            append_bounded(&mut expanded, &child_source).map_err(|error| format!("{}:{}: {error}", canonical.display(), line_index + 1))?;
            if !child_source.is_empty() && !child_source.ends_with('\n') {
                append_bounded(&mut expanded, "\n").map_err(|error| format!("{}:{}: {error}", canonical.display(), line_index + 1))?;
            }
        } else {
            append_bounded(&mut expanded, line).map_err(|error| format!("{}:{}: {error}", canonical.display(), line_index + 1))?;
            append_bounded(&mut expanded, "\n").map_err(|error| format!("{}:{}: {error}", canonical.display(), line_index + 1))?;
        }
    }

    active.pop();
    Ok(expanded)
}

fn parse_module_path(input: &str) -> Result<&str, &'static str> {
    let trimmed = input.trim();
    let input = trimmed.strip_suffix(';').unwrap_or(trimmed).trim();
    let Some(rest) = input.strip_prefix('"') else {
        return Err("expected module path as use \"relative/path.zv\"");
    };
    let Some(end) = rest.find('"') else {
        return Err("unterminated module path string");
    };
    if !rest[end + 1..].trim().is_empty() {
        return Err("unexpected text after module path");
    }
    let path = &rest[..end];
    if path.is_empty() {
        return Err("module path cannot be empty");
    }
    if path.contains('\\') {
        return Err("module paths must use forward slashes");
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::load;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock must be after UNIX epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "zenvora-module-loader-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create temporary test directory");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn rejects_oversized_module_source_before_unbounded_reading() {
        let temp = TempDir::new();
        let path = temp.0.join("large.zv");
        fs::write(&path, "x".repeat(super::MAX_MODULE_SOURCE_BYTES as usize + 1)).unwrap();
        let error = load(&path).unwrap_err();
        assert!(error.contains("module source exceeds safety limit"), "{error}");
    }

    #[test]
    fn expanded_source_size_is_bounded() {
        let mut expanded = "x".repeat(super::MAX_EXPANDED_SOURCE_BYTES);
        let error = super::append_bounded(&mut expanded, "y").unwrap_err();
        assert!(error.contains("expanded module source exceeds safety limit"), "{error}");
        assert_eq!(expanded.len(), super::MAX_EXPANDED_SOURCE_BYTES);
    }

    #[test]
    fn expands_relative_modules_and_ignores_duplicate_imports() {
        let temp = TempDir::new();
        fs::create_dir_all(temp.0.join("lib")).unwrap();
        fs::write(temp.0.join("main.zv"), "use \"lib/math.zv\"\nuse \"lib/math.zv\"\nshow add(2, 3)\n").unwrap();
        fs::write(temp.0.join("lib/math.zv"), "fn add(a, b) {\n return a + b\n}\n").unwrap();

        let source = load(&temp.0.join("main.zv")).unwrap();
        assert_eq!(source.matches("fn add").count(), 1);
        assert!(source.contains("show add(2, 3)"));
    }

    #[test]
    fn reports_cyclic_imports() {
        let temp = TempDir::new();
        fs::write(temp.0.join("a.zv"), "use \"b.zv\"\n").unwrap();
        fs::write(temp.0.join("b.zv"), "use \"a.zv\"\n").unwrap();

        let error = load(&temp.0.join("a.zv")).unwrap_err();
        assert!(error.contains("cyclic module import"));
    }

    #[test]
    fn reports_missing_modules_with_import_location() {
        let temp = TempDir::new();
        fs::write(temp.0.join("main.zv"), "use \"missing.zv\"\n").unwrap();

        let error = load(&temp.0.join("main.zv")).unwrap_err();
        assert!(error.contains("main.zv:1"));
        assert!(error.contains("missing.zv"));
    }

    #[test]
    fn rejects_malformed_use_directives() {
        let temp = TempDir::new();
        fs::write(temp.0.join("main.zv"), "use lib.zv\n").unwrap();

        assert!(load(&temp.0.join("main.zv")).unwrap_err().contains("expected module path"));
    }
}
