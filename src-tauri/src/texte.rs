//! Oberflächentexte der Hülle (Menü, Dialoge, Mitteilungen) — Deutsch, Englisch
//! wenn das System auf Englisch steht. Die Atlas-Oberfläche selbst bringt ihre
//! eigene Sprachwahl mit.

use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sprache {
    De,
    En,
}

pub fn sprache() -> Sprache {
    static S: OnceLock<Sprache> = OnceLock::new();
    *S.get_or_init(|| match sys_locale::get_locale() {
        Some(l) if l.to_lowercase().starts_with("de") => Sprache::De,
        Some(_) => Sprache::En,
        None => Sprache::De,
    })
}

/// `t("Deutsch", "English")`
pub fn t(de: &'static str, en: &'static str) -> &'static str {
    match sprache() {
        Sprache::De => de,
        Sprache::En => en,
    }
}
