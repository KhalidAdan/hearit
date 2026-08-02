//! Where hearit's companions live — resolved at RUNTIME, so the same exe
//! works from the repo, from a CI artifact, or from an installer. Nothing
//! is compiled in. Resolution order:
//!
//! 1. Env var override (HEARIT_SIDECAR)
//! 2. The installed layout: next to hearit.exe
//! 3. The repo layout: walk up from the exe (target\debug or
//!    target\release are inside the repo) until a candidate exists
//!
//! Same file as sayit's paths.rs because it earned its keep there; the
//! only difference is which companions exist: the koko exe, the ONNX
//! model, and the voices data. All three are passed to the sidecar
//! explicitly (koko's -m and -d flags), so nothing depends on the cwd.

use std::path::{Path, PathBuf};

/// Walk from `start` up through its ancestors, returning the first
/// candidate that exists. Pure enough to unit test.
fn find_from(start: &Path, candidates: &[&str]) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        for rel in candidates {
            let c = d.join(rel);
            if c.exists() {
                return Some(c);
            }
        }
        dir = d.parent();
    }
    None
}

fn find(env_key: &str, candidates: &[&str]) -> Option<PathBuf> {
    if let Ok(overridden) = std::env::var(env_key) {
        return Some(PathBuf::from(overridden));
    }
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    find_from(&exe_dir, candidates)
}

pub fn sidecar_exe() -> Result<PathBuf, String> {
    find(
        "HEARIT_SIDECAR",
        &[r"sidecar\koko.exe", r"sidecar\kokoros\koko.exe"],
    )
    .ok_or_else(|| {
        "koko.exe not found — set HEARIT_SIDECAR or place sidecar\\ next to hearit.exe (docs/sidecar.md)"
            .into()
    })
}

pub fn model() -> Result<PathBuf, String> {
    find("HEARIT_MODEL", &[r"sidecar\checkpoints\kokoro-v1.0.onnx"]).ok_or_else(|| {
        "kokoro-v1.0.onnx not found — set HEARIT_MODEL or place sidecar\\checkpoints\\ next to hearit.exe (docs/sidecar.md)"
            .into()
    })
}

pub fn voices() -> Result<PathBuf, String> {
    find("HEARIT_VOICES", &[r"sidecar\data\voices-v1.0.bin"]).ok_or_else(|| {
        "voices-v1.0.bin not found — set HEARIT_VOICES or place sidecar\\data\\ next to hearit.exe (docs/sidecar.md)"
            .into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_candidate_in_an_ancestor() {
        let root = std::env::temp_dir().join("hearit-paths-test");
        let deep = root.join("target").join("release");
        std::fs::create_dir_all(root.join("stuff")).unwrap();
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(root.join("stuff").join("thing.bin"), b"x").unwrap();

        let found = find_from(&deep, &[r"stuff\thing.bin"]);
        assert_eq!(found, Some(root.join("stuff").join("thing.bin")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn returns_none_when_nothing_exists() {
        let lonely = std::env::temp_dir().join("hearit-paths-empty");
        std::fs::create_dir_all(&lonely).unwrap();
        assert_eq!(find_from(&lonely, &["definitely-not-real.xyz"]), None);
        let _ = std::fs::remove_dir_all(&lonely);
    }
}
