use super::*;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Freshness {
    pub(crate) root: PathBuf,
    pub(super) source_digest: Option<String>,
    pub(super) files: BTreeMap<PathBuf, String>,
    pub(super) generator_version: String,
    pub(super) generator_revision: String,
    pub(super) configuration: Option<String>,
}
pub(crate) fn check(root: &Path) -> anyhow::Result<()> {
    let path = root.join(".lenso/contracts/freshness.json");
    if !path.exists() {
        ensure!(
            !root.join(".lenso/contracts").exists(),
            "contract freshness evidence is missing; rebuild before check/pack"
        );
        return Ok(());
    }
    let evidence: Vec<Freshness> = serde_json::from_slice(&fs::read(path)?)?;
    for contract in evidence {
        ensure!(
            contract.generator_version == lenso_contract_codegen_next::GENERATOR_VERSION,
            "contract generator cohort changed; rebuild before check/pack"
        );
        ensure!(
            contract.generator_revision == lenso_contract_codegen_next::GENERATOR_REVISION,
            "contract generator source changed; rebuild before check/pack"
        );
        let configuration = root.join("lenso.toml");
        let current = if configuration.exists() {
            Some(file_digest(&configuration)?)
        } else {
            None
        };
        ensure!(
            current == contract.configuration,
            "contract selection configuration changed; rebuild before check/pack"
        );
        if let Some(expected) = contract.source_digest {
            ensure!(
                super::super::local_host::input_digest(&contract.root)? == expected,
                "stale contract source: {}; rebuild before check/pack",
                contract.root.display()
            );
        }
        for (path, expected) in contract.files {
            ensure!(
                file_digest(&path)? == expected,
                "stale contract input/projection: {}; rebuild before check/pack",
                path.display()
            );
        }
    }
    Ok(())
}
pub(super) fn file_digest(path: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(fs::read(path)?) {
        write!(output, "{byte:02x}").expect("String write");
    }
    Ok(output)
}
