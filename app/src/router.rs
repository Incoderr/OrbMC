use crate::features::settings::Locale;
use gpui_kit::assets::IconName;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Home,
    Instances,
    /// Detail page of one build: installed mods with enable/disable.
    InstanceDetails(crate::features::instances::InstanceId),
    /// Mods, modpacks, texture packs and shaders; the kind is switched on the page.
    Catalog,
    Settings,
    Skins,
    ModDetails,
}

impl Page {
    pub const LIBRARY: [Self; 4] = [Self::Home, Self::Instances, Self::Catalog, Self::Skins];

    pub fn title(self, locale: Locale) -> &'static str {
        match self {
            Self::Home => locale.text("Главная", "Home"),
            Self::Instances => locale.text("Сборки", "Builds"),
            Self::InstanceDetails(_) => locale.text("Сборка", "Build"),
            Self::Catalog => locale.text("Каталог", "Catalog"),
            Self::Settings => locale.text("Настройки", "Settings"),
            Self::Skins => locale.text("Скины", "Skins"),
            Self::ModDetails => locale.text("Проект", "Project"),
        }
    }

    pub fn element_id(self) -> &'static str {
        match self {
            Self::Home => "nav-home",
            Self::Instances => "nav-instances",
            Self::InstanceDetails(_) => "nav-instance-details",
            Self::Catalog => "nav-catalog",
            Self::Settings => "nav-settings",
            Self::Skins => "nav-skins",
            Self::ModDetails => "nav-mod-details",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Self::Home => IconName::House,
            Self::Instances => IconName::Box,
            Self::InstanceDetails(_) => IconName::Box,
            Self::Catalog => IconName::Puzzle,
            Self::Settings => IconName::Settings,
            Self::Skins => IconName::Paintbrush,
            Self::ModDetails => IconName::BookOpen,
        }
    }
}
