//! Retain only the source files owned by this Host generator.
use std::{fs, path::Path};

use anyhow::Context;

const BASE_FILES: [&str; 5] = [
    "Cargo.toml",
    "Cargo.lock",
    "src/main.rs",
    "src/plugin_links.rs",
    "build.rs",
];

pub(super) const TERMINAL_FILES: [&str; 4] = [
    "src/terminal/mod.rs",
    "src/terminal/parser.rs",
    "src/terminal/command.rs",
    "src/terminal/provider.rs",
];

/// Call before emitting each generation, including when terminal is disabled.
/// This directory belongs to the generator; a reused cache is not an inventory.
pub(super) fn clear_terminal(root: &Path) -> anyhow::Result<()> {
    let directory = root.join("src/terminal");
    match fs::remove_dir_all(&directory) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error)
            .with_context(|| format!("clear generated terminal source {}", directory.display())),
    }
}

/// Copy the required base and current optional terminal files, never the cache
/// tree. The caller must clear the generated terminal directory before emission.
pub(super) fn copy(generated: &Path, provenance: &Path) -> anyhow::Result<()> {
    clear_terminal(provenance)?;
    fs::create_dir_all(provenance.join("src"))?;
    for file in BASE_FILES {
        fs::copy(generated.join(file), provenance.join(file))
            .with_context(|| format!("retain generated Host source {file}"))?;
    }
    for file in TERMINAL_FILES {
        let source = generated.join(file);
        if source.try_exists()? {
            fs::create_dir_all(provenance.join("src/terminal"))?;
            fs::copy(&source, provenance.join(file))
                .with_context(|| format!("retain generated Host source {file}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_base(root: &Path, terminal: bool) {
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"retained-terminal-host\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            root.join("Cargo.lock"),
            "version = 4\n\n[[package]]\nname = \"retained-terminal-host\"\nversion = \"0.0.0\"\n",
        )
        .unwrap();
        fs::write(root.join("build.rs"), "fn main() {}\n").unwrap();
        let source = if terminal {
            "mod terminal;\npub mod shared_command { pub fn value() -> u8 { 3 } }\npub mod shared_provider { pub fn value() -> u8 { 4 } }\nfn main() { assert_eq!(terminal::run(), 7); }\n"
        } else {
            "fn main() {}\n"
        };
        fs::write(root.join("src/main.rs"), source).unwrap();
        fs::write(root.join("src/plugin_links.rs"), "{}\n").unwrap();
    }

    fn write_terminal(root: &Path, command: bool, provider: bool) {
        fs::create_dir_all(root.join("src/terminal")).unwrap();
        let mut module = "mod parser;\npub fn run() -> u8 { parser::run() }\n".to_owned();
        for (name, fallback, value) in [("command", command, 3), ("provider", provider, 4)] {
            if fallback {
                module.push_str(&format!("pub mod {name};\n"));
                fs::write(
                    root.join(format!("src/terminal/{name}.rs")),
                    format!("pub fn value() -> u8 {{ {value} }}\n"),
                )
                .unwrap();
            } else {
                module.push_str(&format!("pub use crate::shared_{name} as {name};\n"));
            }
        }
        fs::write(root.join("src/terminal/mod.rs"), module).unwrap();
        fs::write(
            root.join("src/terminal/parser.rs"),
            "pub fn run() -> u8 { super::command::value() + super::provider::value() }\n",
        )
        .unwrap();
    }

    fn rebuild(provenance: &Path, target: &Path) {
        let output = crate::app::cargo_command()
            .args(["build", "--locked", "--offline", "--manifest-path"])
            .arg(provenance.join("Cargo.toml"))
            .env("CARGO_TARGET_DIR", target)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "retained generated Host failed to rebuild: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn selected_source_module_links_rebuild_without_generation_cache() {
        let generated = tempfile::tempdir().unwrap();
        let stage = tempfile::tempdir().unwrap();
        write_base(generated.path(), false);
        fs::write(generated.path().join("src/main.rs"), r#"
mod local_plugin_0 {
    pub mod left { pub fn link_plugin() { crate::LINKS.fetch_or(1, std::sync::atomic::Ordering::SeqCst); } }
    pub mod right { pub fn link_plugin() { crate::LINKS.fetch_or(2, std::sync::atomic::Ordering::SeqCst); } }
}
static LINKS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
fn main() {
    include!("plugin_links.rs");
    assert_eq!(LINKS.load(std::sync::atomic::Ordering::SeqCst), 3);
}
"#).unwrap();
        fs::write(
            generated.path().join("src/plugin_links.rs"),
            "{ local_plugin_0::left::link_plugin(); local_plugin_0::right::link_plugin(); }\n",
        )
        .unwrap();
        let provenance = stage.path().join(".lenso/generated-host");
        copy(generated.path(), &provenance).unwrap();
        fs::remove_dir_all(generated.path()).unwrap();
        assert!(
            super::super::distribution_file_paths(stage.path(), &[])
                .contains(&".lenso/generated-host/src/plugin_links.rs".into())
        );
        let target = stage.path().join("target");
        rebuild(&provenance, &target);
        assert!(
            crate::app::build_command(target.join("debug/retained-terminal-host"))
                .status()
                .unwrap()
                .success()
        );
    }

    #[test]
    fn retained_terminal_source_rebuilds_without_generation_cache() {
        let generated = tempfile::tempdir().unwrap();
        let stage = tempfile::tempdir().unwrap();
        clear_terminal(generated.path()).unwrap();
        write_base(generated.path(), true);
        write_terminal(generated.path(), true, true);
        fs::write(
            generated.path().join("src/local_business_snapshot.rs"),
            "compile_error!(\"stale business implementation\");\n",
        )
        .unwrap();
        let provenance = stage.path().join(".lenso/generated-host");
        copy(generated.path(), &provenance).unwrap();
        fs::remove_dir_all(generated.path()).unwrap();
        rebuild(&provenance, &stage.path().join("target"));
        assert!(!provenance.join("src/local_business_snapshot.rs").exists());
        for file in TERMINAL_FILES {
            assert!(provenance.join(file).is_file(), "missing {file}");
        }
    }

    #[test]
    fn cache_reuse_discards_replaced_fallbacks_and_disabled_terminal() {
        let generated = tempfile::tempdir().unwrap();
        let stage = tempfile::tempdir().unwrap();
        let provenance = stage.path().join(".lenso/generated-host");
        // Reuse both directories, first with every fallback, then with either
        // contract supplied through an alias, both aliases, and no terminal.
        for selection in [
            Some((true, true)),
            Some((true, false)),
            Some((false, true)),
            Some((false, false)),
            None,
        ] {
            clear_terminal(generated.path()).unwrap();
            write_base(generated.path(), selection.is_some());
            if let Some((command, provider)) = selection {
                write_terminal(generated.path(), command, provider);
            }
            copy(generated.path(), &provenance).unwrap();
            let expected = match selection {
                Some((command, provider)) => [true, true, command, provider],
                None => [false; 4],
            };
            for (file, exists) in TERMINAL_FILES.into_iter().zip(expected) {
                assert_eq!(generated.path().join(file).exists(), exists, "cache {file}");
                assert_eq!(provenance.join(file).exists(), exists, "retained {file}");
                assert_eq!(
                    super::super::distribution_file_paths(stage.path(), &[])
                        .contains(&format!(".lenso/generated-host/{file}")),
                    exists,
                    "distribution hash inventory {file}"
                );
            }
            rebuild(&provenance, &stage.path().join("target"));
        }
    }
}
