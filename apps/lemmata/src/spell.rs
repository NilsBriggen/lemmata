//! Spell checking. The checking itself is lemmata-spell's; this side knows the languages.

/// Languages with a dictionary, bundled or downloaded on demand (lemmata-spell's manifest), in
/// the order the Svelte app listed them.
pub const LANGUAGES: [(&str, &str); 19] = [
    ("en", "English (US)"),
    ("en-GB", "English (UK)"),
    ("de", "Deutsch"),
    ("de-CH", "Deutsch (Schweiz)"),
    ("fr", "Français"),
    ("es", "Español"),
    ("it", "Italiano"),
    ("pt-PT", "Português (Portugal)"),
    ("pt-BR", "Português (Brasil)"),
    ("nl", "Nederlands"),
    ("pl", "Polski"),
    ("sv", "Svenska"),
    ("da", "Dansk"),
    ("nb", "Norsk bokmål"),
    ("cs", "Čeština"),
    ("ru", "Русский"),
    ("uk", "Українська"),
    ("tr", "Türkçe"),
    ("el", "Ελληνικά"),
];
