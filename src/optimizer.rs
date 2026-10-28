//! Politique d'optimisation : ciblée, jamais aveugle.
use crate::win_api::{self, ProcInfo};

/// Processus qu'on ne touche jamais (comparaison exacte, insensible à la casse).
const PROTECTED: &[&str] = &[
    "system", "registry", "memcompression", "smss.exe", "csrss.exe", "wininit.exe",
    "services.exe", "lsass.exe", "winlogon.exe", "dwm.exe", "explorer.exe",
    "fontdrvhost.exe", "memguard.exe",
];

#[derive(Clone)]
pub struct Policy {
    /// Charge mémoire (%) à partir de laquelle l'auto-optimisation se déclenche
    pub threshold_pct: u32,
    /// Ne traiter que les processus dépassant ce working set (Mo)
    pub min_ws_mb: f32,
    /// Noms exacts à ne jamais toucher (en plus de PROTECTED)
    pub whitelist: Vec<String>,
    /// Purger aussi la Standby List (basse priorité uniquement)
    pub purge_standby: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            threshold_pct: 80,
            min_ws_mb: 80.0,
            whitelist: Vec::new(),
            purge_standby: true,
        }
    }
}

pub fn parse_whitelist(text: &str) -> Vec<String> {
    text.split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn is_protected(name: &str) -> bool {
    let n = name.to_lowercase();
    PROTECTED.iter().any(|p| *p == n)
}

#[derive(Default, Clone)]
pub struct Report {
    pub processes_lightened: usize,
    pub standby_purged: bool,
    /// Variation de la RAM disponible (Mo), mesurée avant/après
    pub delta_avail_mb: f64,
}

/// Allège les processus d'arrière-plan lourds. Épargne : le processus au
/// premier plan, memguard, les processus protégés et la whitelist.
pub fn run(policy: &Policy, procs: &[ProcInfo], has_privilege: bool) -> Report {
    let before = win_api::memory_stats().avail_mb;
    let fg = win_api::foreground_pid();
    let me = win_api::self_pid();
    let mut report = Report::default();

    for p in procs {
        if p.pid == fg || p.pid == me || p.ws_mb < policy.min_ws_mb || is_protected(&p.name) {
            continue;
        }
        let lname = p.name.to_lowercase();
        if policy.whitelist.iter().any(|w| *w == lname) {
            continue;
        }
        if win_api::lighten_process(p.pid, true) {
            report.processes_lightened += 1;
        }
    }

    if policy.purge_standby && has_privilege {
        report.standby_purged = win_api::purge_standby(true);
    }

    win_api::trim_self();
    report.delta_avail_mb = win_api::memory_stats().avail_mb - before;
    report
}
