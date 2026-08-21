use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use lenso_app_plan::ModuleInstancePlan;
use lenso_kernel::RuntimeFailure;
use sha2::{Digest, Sha256};

pub(crate) fn resolve_entrypoint(
    working_directory: &Path,
    instance: &ModuleInstancePlan,
) -> Result<PathBuf, RuntimeFailure> {
    let resolve = |path: &str| {
        let path = Path::new(path);
        if path.is_absolute() {
            path.to_owned()
        } else {
            working_directory.join(path)
        }
    };
    let entrypoint = resolve(instance.entrypoint());
    let Some(artifact) = instance.artifact() else {
        return Ok(entrypoint);
    };
    if artifact.locator().contains("://") {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!(
                "Bun Module Instance `{}` has a remote artifact locator; materialize the locked artifact before run",
                instance.instance_key()
            ),
        });
    }
    let artifact_path = resolve(artifact.locator());
    if !artifact_path.is_file() {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!(
                "locked Bun artifact `{}` for Module Instance `{}` does not exist",
                artifact_path.display(),
                instance.instance_key()
            ),
        });
    }
    let actual_digest = sha256_file(&artifact_path)?;
    if actual_digest != artifact.digest() {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!(
                "locked Bun artifact `{}` changed after resolution",
                artifact_path.display()
            ),
        });
    }
    if fs::canonicalize(&artifact_path).ok() != fs::canonicalize(&entrypoint).ok() {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!(
                "Bun entrypoint `{}` is not the exact locked artifact `{}`",
                entrypoint.display(),
                artifact_path.display()
            ),
        });
    }
    Ok(entrypoint)
}

fn sha256_file(path: &Path) -> Result<String, RuntimeFailure> {
    let bytes = fs::read(path).map_err(|error| RuntimeFailure::InvalidResolvedPlan {
        detail: format!(
            "could not read locked Bun artifact `{}`: {error}",
            path.display()
        ),
    })?;
    let digest = Sha256::digest(bytes);
    let mut value = String::with_capacity(7 + digest.len() * 2);
    value.push_str("sha256:");
    for byte in digest {
        let _ = write!(value, "{byte:02x}");
    }
    Ok(value)
}
