use crate::features::settings::Locale;
use gpui_kit::assets::IconName;

pub mod catalog;
pub mod installed;
pub mod translation;

#[derive(Clone, Debug, Default)]
pub struct ModSearchFilters {
    pub game_version: Option<String>,
    pub loader: Option<ModLoaderFilter>,
    pub category: ModCategory,
}

#[cfg(test)]
mod filter_tests {
    use super::*;
    #[test]
    fn filters_reset_pagination_reject_old_responses_and_follow_the_target_build() {
        let mut state = ModsState {
            page_count: 10,
            ..Default::default()
        };
        state.set_page(5);
        let old = state.begin_search().0;
        state.set_category(ModCategory::Magic);
        assert_eq!(state.page(), 1);
        assert!(!state.finish_search(old, catalog::ModCatalogResult::default()));
        state.set_game_version_filter("1.20.1".into());
        state.select_profile(
            uuid::Uuid::new_v4(),
            ModCompatibilityFilter {
                game_version: "1.21.1".into(),
                loader: ModLoaderFilter::Fabric,
            },
        );
        assert_eq!(
            state.search_filters().game_version.as_deref(),
            Some("1.21.1")
        );
        state.reset_filters();
        assert!(state.target_profile().is_none());
        assert!(state.search_filters().game_version.is_none());
        assert!(state.search_filters().loader.is_none());
        assert_eq!(state.category(), ModCategory::All);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModCategory {
    #[default]
    All,
    Adventure,
    Technology,
    Decoration,
    Optimization,
    Libraries,
    Magic,
    Rpg,
}

impl ModCategory {
    pub const ALL: [Self; 8] = [
        Self::All,
        Self::Adventure,
        Self::Technology,
        Self::Decoration,
        Self::Optimization,
        Self::Libraries,
        Self::Magic,
        Self::Rpg,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::All => "mods-filter-all",
            Self::Adventure => "mods-filter-adventure",
            Self::Technology => "mods-filter-technology",
            Self::Decoration => "mods-filter-decoration",
            Self::Optimization => "mods-filter-optimization",
            Self::Libraries => "mods-filter-libraries",
            Self::Magic => "mods-filter-magic",
            Self::Rpg => "mods-filter-rpg",
        }
    }

    pub fn title(self, locale: Locale) -> &'static str {
        match self {
            Self::All => locale.text("Все", "All"),
            Self::Adventure => locale.text("Приключения", "Adventure"),
            Self::Technology => locale.text("Технологии", "Technology"),
            Self::Decoration => locale.text("Декорации", "Decoration"),
            Self::Optimization => locale.text("Оптимизация", "Optimization"),
            Self::Libraries => locale.text("Библиотеки", "Libraries"),
            Self::Magic => locale.text("Магия", "Magic"),
            Self::Rpg => "RPG",
        }
    }

    pub(crate) fn modrinth_slug(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Adventure => Some("adventure"),
            Self::Technology => Some("technology"),
            Self::Decoration => Some("decoration"),
            Self::Optimization => Some("optimization"),
            Self::Libraries => Some("library"),
            Self::Magic => Some("magic"),
            Self::Rpg => Some("adventure"),
        }
    }

    pub(crate) fn curseforge_slug(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Adventure | Self::Rpg => Some("adventure-rpg"),
            Self::Technology => Some("technology"),
            Self::Decoration => Some("cosmetic"),
            Self::Optimization => Some("performance"),
            Self::Libraries => Some("library-api"),
            Self::Magic => Some("magic"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModSort {
    #[default]
    Popular,
    Downloads,
    Relevance,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProjectKind {
    #[default]
    Mod,
    Modpack,
    ResourcePack,
    Shader,
}

impl ProjectKind {
    pub fn title(self, locale: Locale) -> &'static str {
        match self {
            Self::Mod => locale.text("Моды", "Mods"),
            Self::Modpack => locale.text("Модпаки", "Modpacks"),
            Self::ResourcePack => locale.text("Текстурпаки", "Texture packs"),
            Self::Shader => locale.text("Шейдеры", "Shaders"),
        }
    }

    pub(crate) fn modrinth_type(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Modpack => "modpack",
            Self::ResourcePack => "resourcepack",
            Self::Shader => "shader",
        }
    }

    pub(crate) fn curseforge_class_id(self) -> u32 {
        match self {
            Self::Mod => 6,
            Self::Modpack => 4471,
            Self::ResourcePack => 12,
            Self::Shader => 6552,
        }
    }

    pub(crate) fn modrinth_path(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Modpack => "modpack",
            Self::ResourcePack => "resourcepack",
            Self::Shader => "shader",
        }
    }
}

pub const CATALOG_PAGE_SIZE: usize = 24;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModDetailTab {
    #[default]
    Description,
    Files,
    Dependencies,
    Screenshots,
    Similar,
}

impl ModDetailTab {
    pub const ALL: [Self; 5] = [
        Self::Description,
        Self::Files,
        Self::Dependencies,
        Self::Screenshots,
        Self::Similar,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Description => "mod-detail-description",
            Self::Files => "mod-detail-files",
            Self::Dependencies => "mod-detail-dependencies",
            Self::Screenshots => "mod-detail-screenshots",
            Self::Similar => "mod-detail-similar",
        }
    }

    pub fn title(self, locale: Locale) -> &'static str {
        match self {
            Self::Description => locale.text("Описание", "Description"),
            Self::Files => locale.text("Файлы", "Files"),
            Self::Dependencies => locale.text("Зависимости", "Dependencies"),
            Self::Screenshots => locale.text("Скриншоты", "Screenshots"),
            Self::Similar => locale.text("Похожие", "Similar"),
        }
    }
}

impl ModSort {
    pub fn title(self, locale: Locale) -> &'static str {
        match self {
            Self::Popular => locale.text("Популярные", "Popular"),
            Self::Downloads => locale.text("По загрузкам", "Most downloaded"),
            Self::Relevance => locale.text("По совпадению", "Relevance"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModLoaderFilter {
    Vanilla,
    Fabric,
    Forge,
    NeoForge,
}

impl ModLoaderFilter {
    pub fn title(self, locale: Locale) -> &'static str {
        match self {
            Self::Vanilla => locale.text("Ваниль", "Vanilla"),
            Self::Fabric => "Fabric",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
        }
    }

    pub(crate) fn modrinth_slug(self) -> &'static str {
        match self {
            Self::Vanilla => "minecraft",
            Self::Fabric => "fabric",
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
        }
    }

    pub(crate) fn curseforge_id(self) -> u32 {
        match self {
            Self::Vanilla => 0,
            Self::Forge => 1,
            Self::Fabric => 4,
            Self::NeoForge => 6,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModCompatibilityFilter {
    pub game_version: String,
    pub loader: ModLoaderFilter,
}

#[derive(Clone, Copy)]
pub struct ModSample {
    pub name: &'static str,
    pub author: &'static str,
    pub icon: IconName,
    pub icon_background: u32,
    pub icon_foreground: u32,
    pub image_url: Option<&'static str>,
}

pub const SAMPLE_MODS: &[ModSample] = &[
    ModSample {
        name: "JEI (Just Enough Items)",
        author: "mezz",
        icon: IconName::BookOpen,
        icon_background: 0x32834a,
        icon_foreground: 0xffffff,
        image_url: Some(
            "https://cdn.modrinth.com/data/u6dRKJwZ/4a3f18ac0d096c9f8e9176984c44be4e58f94c89_96.webp",
        ),
    },
    ModSample {
        name: "JourneyMap",
        author: "techbrew",
        icon: IconName::Map,
        icon_background: 0x173d36,
        icon_foreground: 0xb7f4d4,
        image_url: None,
    },
    ModSample {
        name: "Create",
        author: "simibubi",
        icon: IconName::Blocks,
        icon_background: 0xb07a37,
        icon_foreground: 0x24180f,
        image_url: None,
    },
    ModSample {
        name: "Sodium",
        author: "JellySquid",
        icon: IconName::Cpu,
        icon_background: 0x39a849,
        icon_foreground: 0xffffff,
        image_url: None,
    },
    ModSample {
        name: "Farmer's Delight",
        author: "vectorwing",
        icon: IconName::BookOpen,
        icon_background: 0x9e6841,
        icon_foreground: 0xfff1d6,
        image_url: None,
    },
    ModSample {
        name: "Lithium",
        author: "CaffeineMC",
        icon: IconName::Cpu,
        icon_background: 0x447b67,
        icon_foreground: 0xe2fff2,
        image_url: None,
    },
    ModSample {
        name: "Xaero's Minimap",
        author: "xaero96",
        icon: IconName::Map,
        icon_background: 0x65549a,
        icon_foreground: 0xf1edff,
        image_url: None,
    },
    ModSample {
        name: "Botania",
        author: "Vazkii",
        icon: IconName::Asterisk,
        icon_background: 0x406d3e,
        icon_foreground: 0xe5ffd3,
        image_url: None,
    },
];

/// Sidebar filter sections that can be collapsed in the catalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FilterSection {
    Version,
    Loader,
    Categories,
    Source,
}

/// Identifies one description translation in the cache maps.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TranslationKey {
    pub project_id: String,
    pub source: catalog::ModSource,
    pub locale: Locale,
}

impl TranslationKey {
    pub fn for_project(project: &catalog::ModProject, locale: Locale) -> Self {
        Self {
            project_id: project.id.clone(),
            source: project.source,
            locale,
        }
    }
}

/// Per-item state of one card description translation. Missing key means
/// "original text" — cards never share global flags.
#[derive(Clone, Debug)]
pub enum TranslationState {
    Fetching,
    Translated(String),
}

#[derive(Debug, Default)]
pub struct ModsState {
    pub installed: installed::InstalledState,
    kind: ProjectKind,
    category: ModCategory,
    sort: ModSort,
    source: catalog::ModSource,
    query: String,
    projects: Vec<catalog::ModProject>,
    total_results: usize,
    page_count: usize,
    page: usize,
    errors: Vec<catalog::CatalogProviderError>,
    loading: bool,
    request_id: u64,
    compatibility_filter: Option<ModCompatibilityFilter>,
    target_profile: Option<uuid::Uuid>,
    /// Sidebar filter sections currently collapsed.
    collapsed_filters: std::collections::HashSet<FilterSection>,
    game_version_filter: Option<String>,
    loader_filter: Option<ModLoaderFilter>,
    selected_mod: Option<catalog::ModProject>,
    full_description: Option<String>,
    description_loading: bool,
    description_error: Option<String>,
    description_request_id: u64,
    detail_tab: ModDetailTab,
    translations: std::collections::HashMap<TranslationKey, TranslationState>,
    /// Mods installed into each build, for the build details page.
    build_mods: std::collections::HashMap<uuid::Uuid, Vec<installed::InstalledModEntry>>,
}

impl ModsState {
    pub fn kind(&self) -> ProjectKind {
        self.kind
    }

    /// Whether a filters sidebar section is currently collapsed.
    pub fn filters_collapsed(&self, section: FilterSection) -> bool {
        self.collapsed_filters.contains(&section)
    }

    /// Toggles a filters sidebar section; returns the new collapsed state.
    pub fn toggle_filters(&mut self, section: FilterSection) -> bool {
        if !self.collapsed_filters.remove(&section) {
            self.collapsed_filters.insert(section);
            true
        } else {
            false
        }
    }

    pub fn set_kind(&mut self, kind: ProjectKind) {
        if self.kind != kind {
            self.kind = kind;
            self.category = ModCategory::All;
            self.page = 0;
            if kind != ProjectKind::Mod {
                self.compatibility_filter = None;
                self.target_profile = None;
            }
            self.invalidate_search();
        }
    }

    pub fn category(&self) -> ModCategory {
        self.category
    }

    pub fn sort(&self) -> ModSort {
        self.sort
    }

    pub fn source(&self) -> catalog::ModSource {
        self.source
    }

    pub fn set_source(&mut self, source: catalog::ModSource) {
        if self.source != source {
            self.source = source;
            self.page = 0;
            self.invalidate_search();
        }
    }

    pub fn set_category(&mut self, category: ModCategory) {
        self.category = category;
        self.page = 0;
        self.invalidate_search();
    }

    pub fn set_sort(&mut self, sort: ModSort) {
        self.sort = sort;
        self.page = 0;
        self.invalidate_search();
    }

    pub fn page(&self) -> usize {
        self.page + 1
    }

    pub fn total_pages(&self) -> usize {
        self.page_count.max(1)
    }

    pub fn set_page(&mut self, page: usize) {
        self.page = page.clamp(1, self.total_pages()) - 1;
        self.invalidate_search();
    }

    pub fn total_results(&self) -> usize {
        self.total_results
    }

    pub fn compatibility_filter(&self) -> Option<&ModCompatibilityFilter> {
        self.compatibility_filter.as_ref()
    }

    pub fn target_profile(&self) -> Option<uuid::Uuid> {
        self.target_profile
    }

    pub fn select_profile(&mut self, id: uuid::Uuid, filter: ModCompatibilityFilter) {
        self.target_profile = Some(id);
        self.set_compatibility_filter(filter);
    }

    pub fn search_filters(&self) -> ModSearchFilters {
        let mut filters = ModSearchFilters {
            game_version: self
                .compatibility_filter
                .as_ref()
                .map(|f| f.game_version.clone())
                .or_else(|| self.game_version_filter.clone()),
            loader: self
                .compatibility_filter
                .as_ref()
                .map(|f| f.loader)
                .or(self.loader_filter),
            category: self.category,
        };
        if self.kind != ProjectKind::Mod {
            filters.loader = None;
            filters.category = ModCategory::All;
        }
        filters
    }

    pub fn set_game_version_filter(&mut self, version: String) {
        self.game_version_filter = (!version.trim().is_empty()).then(|| version.trim().to_owned());
        self.page = 0;
        self.invalidate_search();
    }

    pub fn set_loader_filter(&mut self, loader: Option<ModLoaderFilter>) {
        self.loader_filter = loader;
        self.page = 0;
        self.invalidate_search();
    }

    pub fn reset_filters(&mut self) {
        self.category = ModCategory::All;
        self.source = catalog::ModSource::All;
        self.game_version_filter = None;
        self.loader_filter = None;
        self.clear_compatibility_filter();
        self.page = 0;
        self.invalidate_search();
    }

    pub fn set_compatibility_filter(&mut self, filter: ModCompatibilityFilter) {
        if self.compatibility_filter.as_ref() != Some(&filter) {
            self.compatibility_filter = Some(filter);
            self.page = 0;
            self.invalidate_search();
        }
    }

    pub fn clear_compatibility_filter(&mut self) {
        self.target_profile = None;
        if self.compatibility_filter.take().is_some() {
            self.page = 0;
            self.invalidate_search();
        }
    }

    pub fn set_query(&mut self, query: String) {
        if self.query != query {
            self.query = query;
            self.page = 0;
            self.invalidate_search();
        }
    }

    pub fn select_mod(&mut self, project: catalog::ModProject) -> u64 {
        self.description_request_id = self.description_request_id.wrapping_add(1);
        self.selected_mod = Some(project);
        self.full_description = None;
        self.description_loading = true;
        self.description_error = None;
        self.detail_tab = ModDetailTab::Description;
        self.description_request_id
    }

    pub fn selected_mod(&self) -> Option<&catalog::ModProject> {
        self.selected_mod.as_ref()
    }

    pub fn clear_selected_mod(&mut self) {
        self.description_request_id = self.description_request_id.wrapping_add(1);
        self.selected_mod = None;
        self.full_description = None;
        self.description_loading = false;
        self.description_error = None;
    }

    pub fn full_description(&self) -> Option<&str> {
        self.full_description.as_deref()
    }

    pub fn description_loading(&self) -> bool {
        self.description_loading
    }

    pub fn description_error(&self) -> Option<&str> {
        self.description_error.as_deref()
    }

    pub fn finish_description_load(
        &mut self,
        request_id: u64,
        result: Result<String, catalog::CatalogProviderError>,
    ) -> bool {
        if self.description_request_id != request_id || self.selected_mod.is_none() {
            return false;
        }

        self.description_loading = false;
        match result {
            Ok(description) if !description.trim().is_empty() => {
                self.full_description = Some(description);
                self.description_error = None;
            }
            Ok(_) => {
                self.description_error = Some("The provider returned an empty description.".into());
            }
            Err(error) => {
                self.description_error = Some(error.message);
            }
        }
        true
    }

    pub fn detail_tab(&self) -> ModDetailTab {
        self.detail_tab
    }

    pub fn set_detail_tab(&mut self, tab: ModDetailTab) {
        self.detail_tab = tab;
    }

    /// Mods installed into the given build for the build details page.
    pub fn build_mods(&self, profile: uuid::Uuid) -> &[installed::InstalledModEntry] {
        self.build_mods
            .get(&profile)
            .map(|entries| entries.as_slice())
            .unwrap_or(&[])
    }

    pub fn set_build_mods(
        &mut self,
        profile: uuid::Uuid,
        entries: Vec<installed::InstalledModEntry>,
    ) {
        self.build_mods.insert(profile, entries);
    }

    /// The description shown in the card and its tooltip.
    ///
    /// Returns the translation when it is ready, otherwise the original
    /// provider text. English never requests a translation.
    pub fn card_description(&self, project: &catalog::ModProject, locale: Locale) -> String {
        if locale != Locale::Ru {
            return project.description.clone();
        }
        match self
            .translations
            .get(&TranslationKey::for_project(project, locale))
        {
            Some(TranslationState::Translated(text)) => text.clone(),
            _ => project.description.clone(),
        }
    }

    /// Marks the description as being fetched. Returns `false` when the
    /// translation already exists or is already in flight; then no new
    /// request is needed.
    pub fn begin_translation(&mut self, project: &catalog::ModProject, locale: Locale) -> bool {
        if locale == Locale::En || project.description.trim().is_empty() {
            return false;
        }
        let key = TranslationKey::for_project(project, locale);
        match self.translations.get(&key) {
            Some(TranslationState::Translated(_)) => false,
            Some(TranslationState::Fetching) => false,
            None => {
                self.translations.insert(key, TranslationState::Fetching);
                true
            }
        }
    }

    /// Stores the result of one background translation job.
    pub fn finish_translation(
        &mut self,
        project: &catalog::ModProject,
        locale: Locale,
        result: Result<String, translation::TranslationError>,
    ) -> bool {
        let key = TranslationKey::for_project(project, locale);
        if !matches!(
            self.translations.get(&key),
            Some(TranslationState::Fetching)
        ) {
            return false;
        }
        match result {
            Ok(text) => {
                self.translations
                    .insert(key, TranslationState::Translated(text));
            }
            // On failure keep the original text on the card: fall back by
            // simply forgetting the attempt so a later search can retry.
            Err(_) => {
                self.translations.remove(&key);
            }
        }
        true
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn errors(&self) -> &[catalog::CatalogProviderError] {
        &self.errors
    }

    pub fn begin_search(
        &mut self,
    ) -> (
        u64,
        String,
        ModSort,
        catalog::ModSource,
        ModSearchFilters,
        ProjectKind,
        usize,
    ) {
        self.request_id = self.request_id.wrapping_add(1);
        self.loading = true;
        self.errors.clear();
        (
            self.request_id,
            self.query.clone(),
            self.sort,
            self.source,
            self.search_filters(),
            self.kind,
            self.page(),
        )
    }

    pub fn is_current_request(&self, request_id: u64) -> bool {
        self.request_id == request_id
    }

    pub fn finish_search(&mut self, request_id: u64, result: catalog::ModCatalogResult) -> bool {
        if !self.is_current_request(request_id) {
            return false;
        }
        self.loading = false;
        self.projects = result.projects;
        self.total_results = result.total_results;
        self.page_count = result.page_count;
        self.errors = result.errors;
        true
    }

    pub fn visible_mods(&self) -> Vec<&catalog::ModProject> {
        let mut mods: Vec<_> = self.projects.iter().collect();

        match self.sort {
            // Provider popularity metrics differ (follows on Modrinth and
            // popularity rank on CurseForge), so retain each provider's rank.
            ModSort::Popular => {}
            ModSort::Downloads => mods.sort_by_key(|project| std::cmp::Reverse(project.downloads)),
            ModSort::Relevance => {}
        }

        mods
    }

    fn invalidate_search(&mut self) {
        self.request_id = self.request_id.wrapping_add(1);
        self.loading = false;
        self.errors.clear();
        self.projects.clear();
        self.total_results = 0;
        self.page_count = 0;
    }
}
