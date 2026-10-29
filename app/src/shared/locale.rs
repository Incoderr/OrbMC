//! UI language of the launcher.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Locale {
    #[default]
    Ru,
    En,
}

impl Locale {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Ru => "ru",
            Self::En => "en",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Ru => "Русский",
            Self::En => "English",
        }
    }

    pub fn text(self, russian: &'static str, english: &'static str) -> &'static str {
        match self {
            Self::Ru => russian,
            Self::En => english,
        }
    }
}
