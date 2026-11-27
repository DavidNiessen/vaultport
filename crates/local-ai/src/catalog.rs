// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned catalog of downloadable models.
//!
//! Every entry is pinned to an immutable Hugging Face **commit revision** (never a branch such as
//! `main`), together with the exact file size and SHA-256 of the GGUF file. The downloader
//! refuses anything that does not match, so a compromised mirror, CDN or repository update can
//! never swap the model under the user's feet. Updating a model is a code change that has to go
//! through review.
//!
//! # Provenance of the pinned values
//!
//! The values below were taken from the Hugging Face Hub API on 2026-11-27:
//!
//! * revision: `GET https://huggingface.co/api/models/<repo>/revision/main` → `sha`
//! * size and SHA-256: `GET https://huggingface.co/api/models/<repo>/tree/<revision>` →
//!   `lfs.size` and `lfs.oid` (the Git LFS object id *is* the SHA-256 of the file content)
//!
//! The 0.5B file was additionally downloaded and hashed locally with `sha256sum`, which matched.
//!
//! Only permissively licensed models (Apache-2.0 / MIT) are listed, so that redistributing the
//! application together with a model download feature stays compatible with GPL-3.0-or-later.

use serde::Serialize;

/// Hosting repository of a pinned model file.
const HF_BASE: &str = "https://huggingface.co";

/// Where a model is the recommended default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Phones and tablets: small download, small memory footprint.
    Mobile,
    /// Laptops and desktops.
    Desktop,
}

/// One downloadable, pinned model file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelEntry {
    /// Stable identifier used in settings and on disk.
    pub id: &'static str,
    /// Human-readable name for the settings screen.
    pub display_name: &'static str,
    /// Hugging Face repository (`owner/name`).
    pub repo: &'static str,
    /// Immutable commit revision (40 hex characters) the file is pinned to.
    pub revision: &'static str,
    /// File name inside the repository, also used as the on-disk file name.
    pub file_name: &'static str,
    /// Exact size of the file in bytes.
    pub size: u64,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: &'static str,
    /// SPDX license identifier of the model weights.
    pub license: &'static str,
    /// Link to the license text at the pinned revision.
    pub license_url: &'static str,
    /// Link to the model card.
    pub model_card_url: &'static str,
    /// Device class this model is the default for.
    pub tier: Tier,
    /// Rough resident memory needed for inference with the default context (bytes).
    pub approx_ram: u64,
}

impl ModelEntry {
    /// The exact `resolve` URL of the pinned file.
    pub fn url(&self) -> String {
        format!(
            "{HF_BASE}/{}/resolve/{}/{}",
            self.repo, self.revision, self.file_name
        )
    }

    /// Whether the entry carries a well-formed pinned digest. Entries that do not are refused by
    /// the model manager (fail closed).
    pub fn is_verified(&self) -> bool {
        is_sha256_hex(self.sha256) && is_revision_hex(self.revision)
    }
}

/// Qwen2.5-Coder-1.5B-Instruct, Q4_K_M (official Qwen GGUF build). Desktop default.
pub const QWEN25_CODER_1_5B_Q4_K_M: ModelEntry = ModelEntry {
    id: "qwen2.5-coder-1.5b-instruct-q4_k_m",
    display_name: "Qwen2.5-Coder 1.5B Instruct (Q4_K_M)",
    repo: "Qwen/Qwen2.5-Coder-1.5B-Instruct-GGUF",
    revision: "f86cb2c1fa58255f8052cc32aeede1b7482d4361",
    file_name: "qwen2.5-coder-1.5b-instruct-q4_k_m.gguf",
    size: 1_117_320_768,
    sha256: "cc324af070c2ecbfd324a30884d2f951a7ff756aba85cb811a6ec436933bb046",
    license: "Apache-2.0",
    license_url: "https://huggingface.co/Qwen/Qwen2.5-Coder-1.5B-Instruct-GGUF/blob/f86cb2c1fa58255f8052cc32aeede1b7482d4361/LICENSE",
    model_card_url: "https://huggingface.co/Qwen/Qwen2.5-Coder-1.5B-Instruct-GGUF",
    tier: Tier::Desktop,
    approx_ram: 1_400 * 1024 * 1024,
};

/// Qwen2.5-Coder-0.5B-Instruct, Q4_K_M (official Qwen GGUF build). Mobile default.
pub const QWEN25_CODER_0_5B_Q4_K_M: ModelEntry = ModelEntry {
    id: "qwen2.5-coder-0.5b-instruct-q4_k_m",
    display_name: "Qwen2.5-Coder 0.5B Instruct (Q4_K_M)",
    repo: "Qwen/Qwen2.5-Coder-0.5B-Instruct-GGUF",
    revision: "ebb2015119c907b064c512bf053e945850b5875f",
    file_name: "qwen2.5-coder-0.5b-instruct-q4_k_m.gguf",
    size: 491_400_064,
    sha256: "1d9614638d18024d0fbb36575a15f1302a3adf044df10345688ec4f6e1c4ff32",
    license: "Apache-2.0",
    license_url: "https://huggingface.co/Qwen/Qwen2.5-Coder-0.5B-Instruct-GGUF/blob/ebb2015119c907b064c512bf053e945850b5875f/LICENSE",
    model_card_url: "https://huggingface.co/Qwen/Qwen2.5-Coder-0.5B-Instruct-GGUF",
    tier: Tier::Mobile,
    approx_ram: 700 * 1024 * 1024,
};

/// All models the application may download.
pub const MODELS: &[ModelEntry] = &[QWEN25_CODER_1_5B_Q4_K_M, QWEN25_CODER_0_5B_Q4_K_M];

/// The device class of the platform this binary is compiled for.
pub const fn current_tier() -> Tier {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        Tier::Mobile
    } else {
        Tier::Desktop
    }
}

/// The default model for a device class.
pub fn default_for(tier: Tier) -> &'static ModelEntry {
    match tier {
        Tier::Mobile => &QWEN25_CODER_0_5B_Q4_K_M,
        Tier::Desktop => &QWEN25_CODER_1_5B_Q4_K_M,
    }
}

/// The default model for the platform this binary is compiled for (0.5B on Android/iOS, 1.5B
/// elsewhere).
pub fn default_model() -> &'static ModelEntry {
    default_for(current_tier())
}

/// Looks a model up by its [`ModelEntry::id`].
pub fn find(id: &str) -> Option<&'static ModelEntry> {
    MODELS.iter().find(|m| m.id == id)
}

/// `true` for exactly 64 lowercase hex characters.
pub(crate) fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// `true` for a full 40 character lowercase hex Git commit id.
pub(crate) fn is_revision_hex(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::UrlPolicy;
    use std::collections::HashSet;

    #[test]
    fn entries_are_pinned_and_verified() {
        for m in MODELS {
            assert!(m.is_verified(), "{} is not verified", m.id);
            assert!(
                is_revision_hex(m.revision),
                "{}: revision not a commit id",
                m.id
            );
            assert_ne!(m.revision, "main");
            assert!(is_sha256_hex(m.sha256), "{}: bad sha256", m.id);
            assert!(m.size > 100 * 1024 * 1024, "{}: implausible size", m.id);
            assert_eq!(
                std::path::Path::new(m.file_name)
                    .extension()
                    .and_then(|e| e.to_str()),
                Some("gguf")
            );
            assert!(
                !m.file_name.contains(['/', '\\']),
                "file name must be a plain name"
            );
            assert!(
                m.id.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
            );
        }
    }

    #[test]
    fn urls_are_https_allow_listed_and_pinned() {
        let policy = UrlPolicy::hugging_face();
        for m in MODELS {
            let url = m.url();
            assert!(url.starts_with("https://huggingface.co/"), "{url}");
            assert!(
                url.contains(&format!("/resolve/{}/", m.revision)),
                "{url} is not pinned"
            );
            assert!(!url.contains("/resolve/main/"));
            let parsed = url::Url::parse(&url).expect("valid url");
            policy.check(&parsed).expect("allow-listed");
            assert!(m.license_url.starts_with("https://huggingface.co/"));
            assert!(m.license_url.contains(m.revision));
            assert!(m.model_card_url.starts_with("https://huggingface.co/"));
        }
    }

    #[test]
    fn licenses_are_gpl_compatible() {
        for m in MODELS {
            assert!(matches!(m.license, "Apache-2.0" | "MIT"), "{}", m.license);
        }
    }

    #[test]
    fn ids_and_files_are_unique() {
        let ids: HashSet<_> = MODELS.iter().map(|m| m.id).collect();
        let files: HashSet<_> = MODELS.iter().map(|m| m.file_name).collect();
        assert_eq!(ids.len(), MODELS.len());
        assert_eq!(files.len(), MODELS.len());
    }

    #[test]
    fn defaults() {
        assert_eq!(default_for(Tier::Mobile).id, QWEN25_CODER_0_5B_Q4_K_M.id);
        assert_eq!(default_for(Tier::Desktop).id, QWEN25_CODER_1_5B_Q4_K_M.id);
        assert_eq!(default_model().tier, current_tier());
        assert!(find(default_model().id).is_some());
        assert!(find("nope").is_none());
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        assert_eq!(current_tier(), Tier::Desktop);
    }

    #[test]
    fn hex_helpers() {
        assert!(is_sha256_hex(&"a".repeat(64)));
        assert!(!is_sha256_hex(&"A".repeat(64)));
        assert!(!is_sha256_hex(&"a".repeat(63)));
        assert!(!is_sha256_hex(&"g".repeat(64)));
        assert!(!is_revision_hex("main"));
    }
}
