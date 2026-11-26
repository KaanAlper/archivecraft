//! Interface text goes through craft-i18n: English is the source text, and translations come
//! from the craft-i18n catalogs when a build embeds them. `tl!` looks up a plain string,
//! `tf!` fills `{name}` placeholders after the lookup.

use std::sync::LazyLock;

/// No catalogs are embedded in this build; every lookup falls back to the English source.
static EMBEDDED: &craft_i18n::Embedded = &[];

pub static I18N: LazyLock<craft_i18n::I18n> = LazyLock::new(|| craft_i18n::I18n::new(EMBEDDED, &["en"]).0);

pub fn lang() -> craft_i18n::Lang {
    craft_i18n::Lang::EN
}

/// Translate a plain interface string.
#[macro_export]
macro_rules! tl {
    ($s:expr) => {
        $crate::i18n::I18N.t($crate::i18n::lang(), $s).to_string()
    };
}

/// Translate, then fill `{name}` placeholders: `tf!("Packing {name}", name = file)`.
#[macro_export]
macro_rules! tf {
    ($s:expr, $($k:ident = $v:expr),+ $(,)?) => {{
        let t = $crate::i18n::I18N.t($crate::i18n::lang(), $s);
        let values: Vec<(&str, String)> = vec![$((stringify!($k), $v.to_string())),+];
        let pairs: Vec<(&str, &str)> = values.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let out = craft_i18n::fmt(t, &pairs);
        out
    }};
}
