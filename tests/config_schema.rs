//! Loads every `configuration.yml` under `RISICO_CONFIGS_DIR` (for example a
//! checkout of the operational chains) with `risico::config`, the schema the
//! runner uses. Skipped when the variable is not set. Paths containing any of
//! the comma-separated `RISICO_CONFIGS_IGNORE` fragments are left out (chains
//! still on an older schema).
#![cfg(feature = "config")]

use std::path::{Path, PathBuf};

use risico::config::ConfigContainer;

fn configs(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() && !name.starts_with('.') {
            configs(&path, found);
        } else if name == "configuration.yml" || name == "configuration.yaml" {
            found.push(path);
        }
    }
}

#[test]
fn operational_configurations_load() {
    let Ok(dir) = std::env::var("RISICO_CONFIGS_DIR") else {
        eprintln!("RISICO_CONFIGS_DIR not set: skipped");
        return;
    };
    let mut found = Vec::new();
    configs(Path::new(&dir), &mut found);
    let ignore = std::env::var("RISICO_CONFIGS_IGNORE").unwrap_or_default();
    let ignore: Vec<&str> = ignore.split(',').filter(|f| !f.is_empty()).collect();
    found.retain(|path| !ignore.iter().any(|f| path.to_string_lossy().contains(f)));
    assert!(!found.is_empty(), "no configuration*.yml under {dir}");
    let failures: Vec<String> = found
        .iter()
        .filter_map(|path| {
            ConfigContainer::from_yaml(path.to_str().unwrap())
                .err()
                .map(|e| format!("{}: {}", path.display(), e))
        })
        .collect();
    eprintln!("{} configurations, {} failed", found.len(), failures.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
