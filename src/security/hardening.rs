use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardeningStatus {
    pub target: String,
    pub stable_release_profile: bool,
    pub hardened_x64_requested: bool,
    pub cet_requested: bool,
    pub stack_protector_requested: bool,
    pub cfi_requested: bool,
    pub warnings: Vec<String>,
}

impl HardeningStatus {
    #[must_use]
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        parts.push(format!("target={}", self.target));
        parts.push(format!("stable_release_profile={}", self.stable_release_profile));
        parts.push(format!("hardened_x64_requested={}", self.hardened_x64_requested));
        parts.push(format!("cet_requested={}", self.cet_requested));
        parts.push(format!("ssp_requested={}", self.stack_protector_requested));
        parts.push(format!("cfi_requested={}", self.cfi_requested));
        parts.join(",")
    }
}

pub fn detect_hardening_status() -> HardeningStatus {
    let target = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);
    let hardened_x64_requested = std::env::var("OO_HARDENED_X64")
        .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    let effective_hardened =
        hardened_x64_requested && std::env::consts::ARCH == "x86_64" && std::env::consts::OS == "linux";

    let mut warnings = Vec::new();
    if hardened_x64_requested && !(std::env::consts::ARCH == "x86_64" && std::env::consts::OS == "linux") {
        warnings.push(
            "hardened-x64 was requested on unsupported host; running with stable defaults".to_string(),
        );
    }

    HardeningStatus {
        target,
        stable_release_profile: true,
        hardened_x64_requested,
        cet_requested: effective_hardened,
        stack_protector_requested: effective_hardened,
        cfi_requested: effective_hardened,
        warnings,
    }
}
