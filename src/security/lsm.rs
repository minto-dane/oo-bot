use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LsmStatus {
    pub active_modules: Vec<String>,
    pub major_lsm: Option<String>,
    pub apparmor_profile: Option<String>,
    pub apparmor_enabled: Option<bool>,
    pub selinux_context: Option<String>,
    pub selinux_enforce: Option<bool>,
    pub smack_active: bool,
    pub tomoyo_active: bool,
    pub yama_ptrace_scope: Option<i32>,
    pub loadpin_active: bool,
    pub safesetid_active: bool,
    pub warnings: Vec<String>,
}

impl LsmStatus {
    #[must_use]
    pub fn active_lsm_summary(&self) -> String {
        if self.active_modules.is_empty() {
            "none".to_string()
        } else {
            self.active_modules.join(",")
        }
    }
}

pub fn detect_lsm_status() -> LsmStatus {
    let mut status = LsmStatus::default();

    match std::fs::read_to_string("/sys/kernel/security/lsm") {
        Ok(raw) => {
            status.active_modules = raw
                .trim()
                .split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(ToString::to_string)
                .collect();
        }
        Err(err) => status.warnings.push(format!("failed to read /sys/kernel/security/lsm: {err}")),
    }

    status.major_lsm = detect_major_lsm(&status.active_modules);
    status.smack_active = status.active_modules.iter().any(|entry| entry == "smack");
    status.tomoyo_active = status.active_modules.iter().any(|entry| entry == "tomoyo");
    status.loadpin_active = status.active_modules.iter().any(|entry| entry == "loadpin");
    status.safesetid_active = status.active_modules.iter().any(|entry| entry == "safesetid");

    if status.active_modules.iter().any(|entry| entry == "apparmor") {
        detect_apparmor(&mut status);
    }

    if status.active_modules.iter().any(|entry| entry == "selinux") {
        detect_selinux(&mut status);
    }

    detect_yama(&mut status);
    status
}

fn detect_major_lsm(active: &[String]) -> Option<String> {
    for candidate in ["apparmor", "selinux", "smack", "tomoyo"] {
        if active.iter().any(|entry| entry == candidate) {
            return Some(candidate.to_string());
        }
    }
    None
}

fn detect_apparmor(status: &mut LsmStatus) {
    match std::fs::read_to_string("/proc/self/attr/apparmor/current") {
        Ok(raw) => {
            let current = raw.trim().to_string();
            if !current.is_empty() {
                status.apparmor_profile = Some(current);
            }
        }
        Err(err) => status
            .warnings
            .push(format!("failed to read AppArmor current profile: {err}")),
    }

    match std::fs::read_to_string("/sys/module/apparmor/parameters/enabled") {
        Ok(raw) => {
            status.apparmor_enabled = Some(raw.trim() == "Y");
        }
        Err(err) => status
            .warnings
            .push(format!("failed to read AppArmor module enabled status: {err}")),
    }
}

fn detect_selinux(status: &mut LsmStatus) {
    let selinux_path = "/proc/self/attr/selinux/current";
    let fallback_path = "/proc/self/attr/current";

    match std::fs::read_to_string(selinux_path) {
        Ok(raw) => {
            let current = raw.trim().to_string();
            if !current.is_empty() {
                status.selinux_context = Some(current);
            }
        }
        Err(primary_err) => match std::fs::read_to_string(fallback_path) {
            Ok(raw) => {
                let current = raw.trim().to_string();
                if !current.is_empty() {
                    status.selinux_context = Some(current);
                }
                status.warnings.push(format!(
                    "failed to read SELinux context from {} ({}); used fallback {}",
                    selinux_path, primary_err, fallback_path
                ));
            }
            Err(fallback_err) => {
                status.warnings.push(format!(
                    "failed to read SELinux context from {} ({}) and fallback {} ({})",
                    selinux_path, primary_err, fallback_path, fallback_err
                ));
            }
        },
    }

    match std::fs::read_to_string("/sys/fs/selinux/enforce") {
        Ok(raw) => {
            status.selinux_enforce = Some(raw.trim() == "1");
        }
        Err(err) => status
            .warnings
            .push(format!("failed to read SELinux enforce status: {err}")),
    }
}

fn detect_yama(status: &mut LsmStatus) {
    match std::fs::read_to_string("/proc/sys/kernel/yama/ptrace_scope") {
        Ok(raw) => match raw.trim().parse::<i32>() {
            Ok(value) => status.yama_ptrace_scope = Some(value),
            Err(err) => status
                .warnings
                .push(format!("failed to parse yama ptrace scope: {err}")),
        },
        Err(err) => status
            .warnings
            .push(format!("failed to read yama ptrace scope: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::detect_major_lsm;

    #[test]
    fn major_lsm_prefers_known_order() {
        let active = vec!["yama".to_string(), "selinux".to_string(), "loadpin".to_string()];
        assert_eq!(detect_major_lsm(&active), Some("selinux".to_string()));
    }
}
