use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Project {
    name: String,
    entry: PathBuf,
    output: PathBuf,
}

fn parse_manifest(text: &str) -> Result<Project, String> {
    let mut name = None;
    let mut entry = None;
    let mut output = None;
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let (key, value) = line.split_once('=')
            .ok_or_else(|| format!("manifest line {}: expected key = value", index + 1))?;
        let key = key.trim();
        let value = value.trim().trim_matches('"');
        if value.is_empty() { return Err(format!("manifest line {}: value cannot be empty", index + 1)); }
        match key {
            "name" if name.is_none() => name = Some(value.to_string()),
            "entry" if entry.is_none() => entry = Some(PathBuf::from(value)),
            "output" if output.is_none() => output = Some(PathBuf::from(value)),
            "name" | "entry" | "output" => return Err(format!("manifest line {}: duplicate key '{key}'", index + 1)),
            _ => return Err(format!("manifest line {}: unknown key '{key}'", index + 1)),
        }
    }
    let name = name.ok_or("manifest is missing 'name'")?;
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("project name may contain only ASCII letters, digits, '-' and '_'".into());
    }
    let entry = entry.unwrap_or_else(|| PathBuf::from("main.zv"));
    let output = output.unwrap_or_else(|| PathBuf::from("build/main.zvb"));
    for (label, path) in [("entry", &entry), ("output", &output)] {
        if path.is_absolute() || path.components().any(|part| matches!(part, Component::ParentDir | Component::RootDir | Component::Prefix(_))) {
            return Err(format!("'{label}' path must stay inside the project directory"));
        }
    }
    if output == Path::new("Zenvora.toml") || output == entry {
        return Err("output path must not overwrite the manifest or source entry".into());
    }
    Ok(Project { name, entry, output })
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        println!("Zenvora Build (zvbuild)\nUsage:\n  zvbuild init [project-dir]\n  zvbuild build [project-dir]\n  zvbuild run [project-dir]\n  zvbuild clean [project-dir]\n\nProject manifest: Zenvora.toml (name, entry, output).");
        return Ok(());
    }
    let command = args[0].as_str();
    let root = args.get(1).map(PathBuf::from).unwrap_or(env::current_dir().map_err(|e| e.to_string())?);
    match command {
        "init" => {
            if root.exists() && !root.is_dir() { return Err(format!("project path is not a directory: {}", root.display())); }
            fs::create_dir_all(&root).map_err(|e| format!("cannot create project directory: {e}"))?;
            let manifest = root.join("Zenvora.toml");
            if manifest.exists() { return Err(format!("{} already exists; refusing to overwrite", manifest.display())); }
            let name = root.file_name().and_then(|v| v.to_str()).filter(|v| !v.is_empty()).unwrap_or("zenvora-app");
            if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                return Err("project directory name must contain only ASCII letters, digits, '-' and '_'".into());
            }
            let source = root.join("main.zv");
            if source.exists() { return Err(format!("{} already exists; refusing to overwrite", source.display())); }
            fs::write(&manifest, format!("name = \"{name}\"\nentry = \"main.zv\"\noutput = \"build/main.zvb\"\n"))
                .map_err(|e| format!("cannot write manifest: {e}"))?;
            fs::write(&source, "show \"Hello from Zenvora Build!\"\nshow 6 * 7\n")
                .map_err(|e| format!("cannot write starter program: {e}"))?;
            println!("Initialized Zenvora project '{}' at {}", name, root.display());
            println!("Next: zvbuild build {}", root.display());
            Ok(())
        }
        "build" => {
            let manifest_path = root.join("Zenvora.toml");
            let manifest = fs::read_to_string(&manifest_path)
                .map_err(|e| format!("cannot read {}: {e}", manifest_path.display()))?;
            let project = parse_manifest(&manifest)?;
            let source = root.join(&project.entry);
            if !source.is_file() { return Err(format!("entry source does not exist: {}", source.display())); }
            let output = root.join(&project.output);
            if let Some(parent) = output.parent() { fs::create_dir_all(parent).map_err(|e| format!("cannot create build directory: {e}"))?; }
            let compiler = env::var_os("ZENVORA_COMPILER").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("zv"));
            let check = Command::new(&compiler).arg("--check").arg(&source).status()
                .map_err(|e| format!("cannot start compiler '{}': {e}; set ZENVORA_COMPILER to the zv executable path", compiler.display()))?;
            if !check.success() { return Err(format!("compile check failed for project '{}'", project.name)); }
            let emit = Command::new(&compiler).arg("--emit-bytecode").arg(&source).arg(&output).status()
                .map_err(|e| format!("cannot start compiler '{}': {e}", compiler.display()))?;
            if !emit.success() { return Err(format!("bytecode generation failed for project '{}'", project.name)); }
            println!("Build succeeded: {}", project.name);
            println!("Source: {}", source.display());
            println!("Artifact: {}", output.display());
            Ok(())
        }
        "run" => {
            let manifest_path = root.join("Zenvora.toml");
            let manifest = fs::read_to_string(&manifest_path)
                .map_err(|e| format!("cannot read {}: {e}", manifest_path.display()))?;
            let project = parse_manifest(&manifest)?;
            let source = root.join(&project.entry);
            if !source.is_file() { return Err(format!("entry source does not exist: {}", source.display())); }
            let output = root.join(&project.output);
            if let Some(parent) = output.parent() { fs::create_dir_all(parent).map_err(|e| format!("cannot create build directory: {e}"))?; }
            let compiler = env::var_os("ZENVORA_COMPILER").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("zv"));
            let check = Command::new(&compiler).arg("--check").arg(&source).status()
                .map_err(|e| format!("cannot start compiler '{}': {e}; set ZENVORA_COMPILER to the zv executable path", compiler.display()))?;
            if !check.success() { return Err(format!("compile check failed for project '{}'", project.name)); }
            let emit = Command::new(&compiler).arg("--emit-bytecode").arg(&source).arg(&output).status()
                .map_err(|e| format!("cannot start compiler '{}': {e}", compiler.display()))?;
            if !emit.success() { return Err(format!("bytecode generation failed for project '{}'", project.name)); }
            let execution = Command::new(&compiler).arg("--run-bytecode").arg(&output).status()
                .map_err(|e| format!("cannot start compiler '{}': {e}", compiler.display()))?;
            if !execution.success() { return Err(format!("program execution failed for project '{}'", project.name)); }
            Ok(())
        }
        "clean" => {
            let manifest_path = root.join("Zenvora.toml");
            let manifest = fs::read_to_string(&manifest_path)
                .map_err(|e| format!("cannot read {}: {e}", manifest_path.display()))?;
            let project = parse_manifest(&manifest)?;
            let output = root.join(&project.output);
            if output.exists() {
                fs::remove_file(&output).map_err(|e| format!("cannot remove artifact {}: {e}", output.display()))?;
                println!("Removed {}", output.display());
            } else {
                println!("Nothing to clean; artifact does not exist.");
            }
            Ok(())
        }
        _ => Err(format!("unknown command '{command}'; run 'zvbuild --help'")),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => { eprintln!("zvbuild: {error}"); ExitCode::FAILURE }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_manifest;
    #[test]
    fn parses_defaults_and_explicit_paths() {
        let p = parse_manifest("name = \"demo\"\n").unwrap();
        assert_eq!(p.name, "demo");
        assert_eq!(p.entry.to_str(), Some("main.zv"));
        assert_eq!(p.output.to_str(), Some("build/main.zvb"));
        let p = parse_manifest("name = \"app-1\"\nentry = \"src/main.zv\"\noutput = \"dist/app.zvb\"\n").unwrap();
        assert_eq!(p.entry.to_str(), Some("src/main.zv"));
        assert_eq!(p.output.to_str(), Some("dist/app.zvb"));
    }
    #[test]
    fn rejects_unsafe_paths_unknown_keys_and_duplicates() {
        assert!(parse_manifest("name = \"../bad\"").is_err());
        assert!(parse_manifest("name = \"ok\"\nentry = \"../escape.zv\"").is_err());
        assert!(parse_manifest("name = \"ok\"\noutput = \"/tmp/out.zvb\"").is_err());
        assert!(parse_manifest("name = \"ok\"\nunknown = \"x\"").is_err());
        assert!(parse_manifest("name = \"a\"\nname = \"b\"").is_err());
    }
}
