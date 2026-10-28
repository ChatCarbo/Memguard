//! Traductions FR / EN. La langue est globale (atomique) pour que le thread de
//! fond et les actions de l'icône puissent aussi produire des textes traduits.
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Fr,
    En,
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn lang() -> Lang {
    if LANG.load(Ordering::Relaxed) == 0 {
        Lang::Fr
    } else {
        Lang::En
    }
}

pub fn set_lang(l: Lang) {
    LANG.store(if l == Lang::Fr { 0 } else { 1 }, Ordering::Relaxed);
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetUserDefaultUILanguage() -> u16;
}

/// Français si l'interface Windows est en français, anglais sinon.
pub fn detect() -> Lang {
    let id = unsafe { GetUserDefaultUILanguage() };
    if id & 0x3FF == 0x0C {
        Lang::Fr
    } else {
        Lang::En
    }
}

/// Lit `--lang=fr` / `--lang=en` dans les arguments, sinon détecte.
pub fn from_args_or_detect() -> Lang {
    for a in std::env::args() {
        match a.to_lowercase().as_str() {
            "--lang=fr" => return Lang::Fr,
            "--lang=en" => return Lang::En,
            _ => {}
        }
    }
    detect()
}

#[derive(Clone, Copy)]
pub enum K {
    Show,
    OptimizeNow,
    Quit,
    Auto,
    Threshold,
    MinPerProcess,
    PurgeStandby,
    Whitelist,
    WhitelistHint,
    Protected,
    Lighten,
    Manual,
    AutoPrefix,
    Purged,
    Intact,
}

pub fn t(k: K) -> &'static str {
    let fr = lang() == Lang::Fr;
    match k {
        K::Show => if fr { "Afficher" } else { "Show" },
        K::OptimizeNow => if fr { "Optimiser maintenant" } else { "Optimize now" },
        K::Quit => if fr { "Quitter" } else { "Quit" },
        K::Auto => if fr { "Auto" } else { "Auto" },
        K::Threshold => if fr { "seuil" } else { "threshold" },
        K::MinPerProcess => if fr { "min / processus" } else { "min / process" },
        K::PurgeStandby => {
            if fr { "Purge Standby (basse prio.)" } else { "Purge Standby (low prio.)" }
        }
        K::Whitelist => if fr { "Whitelist :" } else { "Whitelist:" },
        K::WhitelistHint => {
            if fr {
                "noms exacts séparés par des virgules, ex : code.exe, obs64.exe"
            } else {
                "exact names separated by commas, e.g. code.exe, obs64.exe"
            }
        }
        K::Protected => if fr { "protégé" } else { "protected" },
        K::Lighten => if fr { "Alléger" } else { "Lighten" },
        K::Manual => if fr { "Manuel" } else { "Manual" },
        K::AutoPrefix => if fr { "Auto" } else { "Auto" },
        K::Purged => if fr { "purgée" } else { "purged" },
        K::Intact => if fr { "intacte" } else { "untouched" },
    }
}

/// Unité de mémoire : Mo/Go en français, MB/GB en anglais.
pub fn unit_mb() -> &'static str {
    if lang() == Lang::Fr { "Mo" } else { "MB" }
}

pub fn unit_gb() -> &'static str {
    if lang() == Lang::Fr { "Go" } else { "GB" }
}

pub fn tooltip(load_pct: u32, free: &str) -> String {
    if lang() == Lang::Fr {
        format!("MemGuard : {} % ({} libres)", load_pct, free)
    } else {
        format!("MemGuard: {}% ({} free)", load_pct, free)
    }
}

pub fn stats_line(standby: &str, modified: &str, commit: &str, limit: &str) -> String {
    if lang() == Lang::Fr {
        format!(
            "Standby : {}   Modifiée : {}   Commit : {} / {}",
            standby, modified, commit, limit
        )
    } else {
        format!(
            "Standby: {}   Modified: {}   Commit: {} / {}",
            standby, modified, commit, limit
        )
    }
}

pub fn optimization_report(auto: bool, lightened: usize, purged: bool, delta_mb: f64) -> String {
    let who = if auto { t(K::AutoPrefix) } else { t(K::Manual) };
    let standby = if purged { t(K::Purged) } else { t(K::Intact) };
    if lang() == Lang::Fr {
        format!(
            "{} : {} processus allégés, standby {}, Δ RAM libre {:+.0} Mo",
            who, lightened, standby, delta_mb
        )
    } else {
        format!(
            "{}: {} processes lightened, standby {}, free RAM Δ {:+.0} MB",
            who, lightened, standby, delta_mb
        )
    }
}
