// SPDX-License-Identifier: GPL-3.0-or-later
//! Locale handling: automatic detection from the system locale, with
//! overrides via --lang / MOVARA_LANG.
//!
//! Supported locales: en (default), zh-CN, ja, ko, es, fr, de, pt-BR.

/// map a raw BCP-47-ish locale to the closest supported one
pub fn normalize(raw: &str) -> String {
    let lower = raw.to_lowercase();
    let lang = lower.split(['_', '-']).next().unwrap_or("en");
    match lang {
        "zh" => {
            if lower.contains("tw") || lower.contains("hk") || lower.contains("hant") {
                // traditional chinese falls back to english for now
                "en".to_string()
            } else {
                "zh-CN".to_string()
            }
        }
        "ja" => "ja".to_string(),
        "ko" => "ko".to_string(),
        "es" => "es".to_string(),
        "fr" => "fr".to_string(),
        "de" => "de".to_string(),
        "pt" => {
            // both brazilian and european portuguese map to the pt-BR catalog
            "pt-BR".to_string()
        }
        _ => "en".to_string(),
    }
}

/// POSIX locale environment with standard precedence (encoding suffix
/// stripped); "C"/"POSIX" treated as unset
fn env_locale() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|k| std::env::var(k).ok())
        .map(|v| v.split('.').next().unwrap_or(&v).trim().to_string())
        .filter(|v| !v.is_empty() && v != "C" && v != "POSIX")
}

/// resolve the display locale: explicit arg > env > system > en
pub fn detect(explicit: Option<&str>) -> String {
    if let Some(l) = explicit {
        return normalize(l);
    }
    if let Ok(l) = std::env::var("MOVARA_LANG") {
        if !l.is_empty() {
            return normalize(&l);
        }
    }
    if let Some(l) = env_locale() {
        return normalize(&l);
    }
    match sys_locale::get_locale() {
        Some(l) => normalize(&l),
        None => "en".to_string(),
    }
}

pub fn set_locale_from_args(lang: Option<&str>) {
    let locale = detect(lang);
    rust_i18n::set_locale(locale.as_str());
}
