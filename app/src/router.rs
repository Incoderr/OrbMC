use crate::features::{mods::ProjectKind, settings::Locale};
use gpui_kit::assets::IconName;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Home,
    Instances,
    /// Detail page of one build: installed mods with enable/disable.
    InstanceDetails(crate::features::instances::InstanceId),
    Modpacks,
    Mods,
    ResourcePacks,
    Shaders,
    Settings,
    Skins,
    ModDetails,
}

impl Page {
    pub const LIBRARY: [Self; 7] = [
        Self::Home,
        Self::Instances,
        Self::Modpacks,
        Self::Mods,
        Self::Skins,
        Self::ResourcePacks,
        Self::Shaders,
    ];

    pub fn title(self, locale: Locale) -> &'static str {
        match self {
            Self::Home => locale.text("Главная", "Home"),
            Self::Instances => locale.text("Сборки", "Builds"),
            Self::InstanceDetails(_) => locale.text("Сборка", "Build"),
            Self::Modpacks => locale.text("Модпаки", "Modpacks"),
            Self::Mods => locale.text("Моды", "Mods"),
            Self::ResourcePacks => locale.text("Текстурпаки", "Texture packs"),
            Self::Shaders => locale.text("Шейдеры", "Shaders"),
            Self::Settings => locale.text("Настройки", "Settings"),
            Self::Skins => locale.text("Скины", "Skins"),
            Self::ModDetails => locale.text("Проект", "Project"),
        }
    }

    pub fn for_catalog_kind(kind: ProjectKind) -> Self {
        match kind {
            ProjectKind::Mod => Self::Mods,
            ProjectKind::Modpack => Self::Modpacks,
            ProjectKind::ResourcePack => Self::ResourcePacks,
            ProjectKind::Shader => Self::Shaders,
        }
    }

    pub fn catalog_kind(self) -> Option<ProjectKind> {
        match self {
            Self::Modpacks => Some(ProjectKind::Modpack),
            Self::Mods => Some(ProjectKind::Mod),
            Self::ResourcePacks => Some(ProjectKind::ResourcePack),
            Self::Shaders => Some(ProjectKind::Shader),
            _ => None,
        }
    }

    pub fn element_id(self) -> &'static str {
        match self {
            Self::Home => "nav-home",
            Self::Instances => "nav-instances",
            Self::InstanceDetails(_) => "nav-instance-details",
            Self::Modpacks => "nav-modpacks",
            Self::Mods => "nav-mods",
            Self::ResourcePacks => "nav-resource-packs",
            Self::Shaders => "nav-shaders",
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
            Self::Modpacks => IconName::Blocks,
            Self::Mods => IconName::Puzzle,
            Self::ResourcePacks => IconName::Paintbrush,
            Self::Shaders => IconName::Sun,
            Self::Settings => IconName::Settings,
            Self::Skins => IconName::Paintbrush,
            Self::ModDetails => IconName::BookOpen,
        }
    }
}
