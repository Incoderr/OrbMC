use crate::features::settings::Locale;

pub mod catalog;
pub mod installed;
pub mod translation;
pub mod view;

pub use view::{CatalogEvent, CatalogTarget, CatalogView};

#[derive(Clone, Debug, Default)]
pub struct ModSearchFilters {
    pub game_version: Option<String>,
    pub loader: Option<ModLoaderFilter>,
    pub category: ModCategory,
    pub author: Option<AuthorFilter>,
}

/// "Projects by this author". Modrinth filters by user name; CurseForge
/// needs the author id, so authors picked from a Modrinth project show
/// Modrinth results only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorFilter {
    pub name: String,
    pub curseforge_id: Option<u64>,
}

#[cfg(test)]
mod filter_tests {
    use super::*;

    #[test]
    fn every_kind_lists_its_own_categories_once() {
        for kind in ProjectKind::ALL {
            let categories = ModCategory::for_kind(kind);
            assert!(!categories.is_empty());
            assert!(!categories.contains(&ModCategory::All));
            for (index, category) in categories.iter().enumerate() {
                assert!(!categories[index + 1..].contains(category), "{category:?}");
                assert!(category.modrinth_slug().is_some());
            }
        }
        assert!(ModCategory::for_kind(ProjectKind::Mod).contains(&ModCategory::Worldgen));
        assert!(ModCategory::for_kind(ProjectKind::Shader).contains(&ModCategory::Cartoon));
        assert!(!ModCategory::for_kind(ProjectKind::Shader).contains(&ModCategory::Library));
    }

    #[test]
    fn provider_tags_map_to_categories() {
        assert_eq!(
            ModCategory::from_tag("worldgen"),
            Some(ModCategory::Worldgen)
        );
        assert_eq!(
            ModCategory::from_tag("game-mechanics"),
            Some(ModCategory::GameMechanics)
        );
        // CurseForge slugs and names.
        assert_eq!(
            ModCategory::from_tag("performance"),
            Some(ModCategory::Optimization)
        );
        assert_eq!(
            ModCategory::from_tag("API and Library"),
            Some(ModCategory::Library)
        );
        // Loaders are not categories.
        assert_eq!(ModCategory::from_tag("fabric"), None);
        assert_eq!(ModCategory::All.modrinth_slug(), None);
        assert!(ModCategory::Equipment.matches_curseforge("armor-weapons-tools", ""));
        assert!(!ModCategory::Cursed.matches_curseforge("cursed", "Cursed"));
    }

    #[test]
    fn picking_an_active_filter_again_turns_it_off() {
        let mut state = ModsState::default();
        state.toggle_category(ModCategory::Magic);
        assert_eq!(state.category(), ModCategory::Magic);
        state.toggle_category(ModCategory::Magic);
        assert_eq!(state.category(), ModCategory::All);

        state.toggle_source(catalog::ModSource::Modrinth);
        assert_eq!(state.source(), catalog::ModSource::Modrinth);
        state.toggle_source(catalog::ModSource::Modrinth);
        assert_eq!(state.source(), catalog::ModSource::All);

        state.toggle_loader_filter(Some(ModLoaderFilter::Fabric));
        assert_eq!(state.search_filters().loader, Some(ModLoaderFilter::Fabric));
        state.toggle_loader_filter(Some(ModLoaderFilter::Fabric));
        assert_eq!(state.search_filters().loader, None);
    }
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
            crate::features::instances::InstanceId::new(),
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

    #[test]
    fn gallery_index_clamps_to_the_loaded_gallery() {
        let mut state = ModsState::default();
        state.set_gallery_index(0, 5);
        assert_eq!(state.gallery_index(), 0);
        state.set_gallery_index(3, 5);
        assert_eq!(state.gallery_index(), 3);
        state.set_gallery_index(9, 5);
        assert_eq!(state.gallery_index(), 4);
        // Wrap-around callers pass the raw modulo result; clamping keeps
        // the index inside whatever gallery is currently loaded.
        state.set_gallery_index(0, 0);
        assert_eq!(state.gallery_index(), 0);
        state.set_gallery_index(2, 1);
        assert_eq!(state.gallery_index(), 0);
    }
}

/// Catalog category. The list follows the Modrinth categories (each project
/// kind has its own set, see `for_kind`); CurseForge categories are matched
/// by slug or name where an equivalent exists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModCategory {
    #[default]
    All,
    Adventure,
    Cartoon,
    Challenging,
    Combat,
    Cursed,
    Decoration,
    Economy,
    Equipment,
    Fantasy,
    Food,
    GameMechanics,
    KitchenSink,
    Library,
    Lightweight,
    Magic,
    Management,
    Minigame,
    Mobs,
    Modded,
    Multiplayer,
    Optimization,
    Quests,
    Realistic,
    SemiRealistic,
    Simplistic,
    Social,
    Storage,
    Technology,
    Themed,
    Transportation,
    Tweaks,
    Utility,
    VanillaLike,
    Worldgen,
}

/// What one category is called on the providers and in the interface.
struct CategoryInfo {
    modrinth: &'static str,
    ru: &'static str,
    en: &'static str,
    /// CurseForge category slugs or names that mean the same.
    curseforge: &'static [&'static str],
}

impl ModCategory {
    /// Categories of one project kind, as Modrinth lists them
    /// (`/v2/tag/category`, "categories" header), without `All`.
    pub fn for_kind(kind: ProjectKind) -> &'static [Self] {
        use ModCategory::*;
        match kind {
            ProjectKind::Mod => &[
                Adventure,
                Cursed,
                Decoration,
                Economy,
                Equipment,
                Food,
                GameMechanics,
                Library,
                Magic,
                Management,
                Minigame,
                Mobs,
                Optimization,
                Social,
                Storage,
                Technology,
                Transportation,
                Utility,
                Worldgen,
            ],
            ProjectKind::Modpack => &[
                Adventure,
                Challenging,
                Combat,
                KitchenSink,
                Lightweight,
                Magic,
                Multiplayer,
                Optimization,
                Quests,
                Technology,
            ],
            ProjectKind::ResourcePack => &[
                Combat,
                Cursed,
                Decoration,
                Modded,
                Realistic,
                Simplistic,
                Themed,
                Tweaks,
                Utility,
                VanillaLike,
            ],
            ProjectKind::Shader => &[
                Cartoon,
                Cursed,
                Fantasy,
                Realistic,
                SemiRealistic,
                VanillaLike,
            ],
        }
    }

    fn info(self) -> CategoryInfo {
        let info = |modrinth, ru, en, curseforge| CategoryInfo {
            modrinth,
            ru,
            en,
            curseforge,
        };
        match self {
            Self::All => info("", "Все", "All", &[]),
            Self::Adventure => info(
                "adventure",
                "Приключения",
                "Adventure",
                &["adventure-rpg", "adventure and rpg", "adventure-and-rpg"],
            ),
            Self::Cartoon => info("cartoon", "Мультяшные", "Cartoon", &[]),
            Self::Challenging => info("challenging", "Хардкор", "Challenging", &["hardcore"]),
            Self::Combat => info(
                "combat",
                "Сражения",
                "Combat",
                &["combat-pvp", "combat / pvp"],
            ),
            Self::Cursed => info("cursed", "Проклятые", "Cursed", &[]),
            Self::Decoration => info("decoration", "Декорации", "Decoration", &["cosmetic"]),
            Self::Economy => info("economy", "Экономика", "Economy", &[]),
            Self::Equipment => info(
                "equipment",
                "Снаряжение",
                "Equipment",
                &["armor-weapons-tools", "armor, tools, and weapons"],
            ),
            Self::Fantasy => info("fantasy", "Фэнтези", "Fantasy", &["fantasy"]),
            Self::Food => info("food", "Еда", "Food", &["food"]),
            Self::GameMechanics => {
                info("game-mechanics", "Игровые механики", "Game mechanics", &[])
            }
            Self::KitchenSink => info(
                "kitchen-sink",
                "Всё и сразу",
                "Kitchen sink",
                &["extra-large", "extra large"],
            ),
            Self::Library => info(
                "library",
                "Библиотеки",
                "Library",
                &["library-api", "api and library"],
            ),
            Self::Lightweight => info(
                "lightweight",
                "Лёгкие",
                "Lightweight",
                &["small-light", "small / light"],
            ),
            Self::Magic => info("magic", "Магия", "Magic", &["magic"]),
            Self::Management => info(
                "management",
                "Управление",
                "Management",
                &["server-utility", "server utility"],
            ),
            Self::Minigame => info(
                "minigame",
                "Мини-игры",
                "Minigame",
                &["mini-game", "mini game", "minigames"],
            ),
            Self::Mobs => info("mobs", "Мобы", "Mobs", &["mobs"]),
            Self::Modded => info(
                "modded",
                "Для модов",
                "Modded",
                &["mod-support", "mod support"],
            ),
            Self::Multiplayer => info(
                "multiplayer",
                "Мультиплеер",
                "Multiplayer",
                &["multiplayer"],
            ),
            Self::Optimization => info(
                "optimization",
                "Оптимизация",
                "Optimization",
                &["performance"],
            ),
            Self::Quests => info("quests", "Квесты", "Quests", &["quests"]),
            Self::Realistic => info(
                "realistic",
                "Реалистичные",
                "Realistic",
                &["realistic", "photo-realistic", "photo realistic"],
            ),
            Self::SemiRealistic => {
                info("semi-realistic", "Полуреалистичные", "Semi-realistic", &[])
            }
            Self::Simplistic => info("simplistic", "Простые", "Simplistic", &[]),
            Self::Social => info("social", "Общение", "Social", &[]),
            Self::Storage => info("storage", "Хранение", "Storage", &["storage"]),
            Self::Technology => info(
                "technology",
                "Технологии",
                "Technology",
                &["technology", "tech"],
            ),
            Self::Themed => info("themed", "Тематические", "Themed", &[]),
            Self::Transportation => info(
                "transportation",
                "Транспорт",
                "Transportation",
                &["player-transport", "player transport"],
            ),
            Self::Tweaks => info("tweaks", "Твики", "Tweaks", &[]),
            Self::Utility => info(
                "utility",
                "Утилиты",
                "Utility",
                &["utility-qol", "utility & qol"],
            ),
            Self::VanillaLike => info(
                "vanilla-like",
                "Как в ванилле",
                "Vanilla-like",
                &["vanilla", "traditional"],
            ),
            Self::Worldgen => info(
                "worldgen",
                "Генерация мира",
                "World generation",
                &["world-gen", "world gen"],
            ),
        }
    }

    pub fn id(self) -> String {
        format!("mods-filter-{}", self.modrinth_slug().unwrap_or("all"))
    }

    pub fn title(self, locale: Locale) -> &'static str {
        let info = self.info();
        locale.text(info.ru, info.en)
    }

    pub(crate) fn modrinth_slug(self) -> Option<&'static str> {
        Some(self.info().modrinth).filter(|slug| !slug.is_empty())
    }

    /// Whether a CurseForge category (its slug or name) means this one.
    pub(crate) fn matches_curseforge(self, slug: &str, name: &str) -> bool {
        self.info().curseforge.iter().any(|candidate| {
            candidate.eq_ignore_ascii_case(slug) || candidate.eq_ignore_ascii_case(name)
        })
    }

    /// The category a provider tag stands for: a Modrinth slug, or a
    /// CurseForge slug or name.
    pub(crate) fn from_tag(tag: &str) -> Option<Self> {
        ALL_CATEGORIES.into_iter().find(|category| {
            category
                .modrinth_slug()
                .is_some_and(|slug| slug.eq_ignore_ascii_case(tag))
                || category.matches_curseforge(tag, tag)
        })
    }
}

/// Every category but `All`; `for_kind` decides what is shown where.
const ALL_CATEGORIES: [ModCategory; 34] = [
    ModCategory::Adventure,
    ModCategory::Cartoon,
    ModCategory::Challenging,
    ModCategory::Combat,
    ModCategory::Cursed,
    ModCategory::Decoration,
    ModCategory::Economy,
    ModCategory::Equipment,
    ModCategory::Fantasy,
    ModCategory::Food,
    ModCategory::GameMechanics,
    ModCategory::KitchenSink,
    ModCategory::Library,
    ModCategory::Lightweight,
    ModCategory::Magic,
    ModCategory::Management,
    ModCategory::Minigame,
    ModCategory::Mobs,
    ModCategory::Modded,
    ModCategory::Multiplayer,
    ModCategory::Optimization,
    ModCategory::Quests,
    ModCategory::Realistic,
    ModCategory::SemiRealistic,
    ModCategory::Simplistic,
    ModCategory::Social,
    ModCategory::Storage,
    ModCategory::Technology,
    ModCategory::Themed,
    ModCategory::Transportation,
    ModCategory::Tweaks,
    ModCategory::Utility,
    ModCategory::VanillaLike,
    ModCategory::Worldgen,
];

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
    pub const ALL: [Self; 4] = [Self::Mod, Self::Modpack, Self::ResourcePack, Self::Shader];

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

    pub(crate) fn install_directory(self) -> Option<&'static str> {
        match self {
            Self::ResourcePack => Some("resourcepacks"),
            Self::Shader => Some("shaderpacks"),
            Self::Mod | Self::Modpack => None,
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
}

impl ModDetailTab {
    pub const ALL: [Self; 4] = [
        Self::Description,
        Self::Files,
        Self::Dependencies,
        Self::Screenshots,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Description => "mod-detail-description",
            Self::Files => "mod-detail-files",
            Self::Dependencies => "mod-detail-dependencies",
            Self::Screenshots => "mod-detail-screenshots",
        }
    }

    pub fn title(self, locale: Locale) -> &'static str {
        match self {
            Self::Description => locale.text("Описание", "Description"),
            Self::Files => locale.text("Файлы", "Files"),
            Self::Dependencies => locale.text("Зависимости", "Dependencies"),
            Self::Screenshots => locale.text("Скриншоты", "Screenshots"),
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
    author: Option<AuthorFilter>,
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
    target_profile: Option<crate::features::instances::InstanceId>,
    /// Sidebar filter sections currently collapsed.
    collapsed_filters: std::collections::HashSet<FilterSection>,
    game_version_filter: Option<String>,
    loader_filter: Option<ModLoaderFilter>,
    selected_mod: Option<catalog::ModProject>,
    full_description: Option<String>,
    description_loading: bool,
    description_error: Option<String>,
    description_request_id: u64,
    /// Provider project details fetched after selecting a mod (downloads,
    /// likes, links, release dates, gallery screenshots).
    project_info: Option<catalog::ModProjectInfo>,
    info_loading: bool,
    info_error: Option<String>,
    detail_tab: ModDetailTab,
    /// Position of the selected screenshot in the project gallery.
    gallery_index: usize,
    translations: std::collections::HashMap<TranslationKey, TranslationState>,
    /// Machine translation of descriptions is switched on in the settings.
    translate: bool,
    /// Mods installed into each build, for the build details page.
    build_mods: std::collections::HashMap<
        crate::features::instances::InstanceId,
        Vec<installed::InstalledModEntry>,
    >,
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

    pub fn target_profile(&self) -> Option<crate::features::instances::InstanceId> {
        self.target_profile
    }

    pub fn select_profile(
        &mut self,
        id: crate::features::instances::InstanceId,
        filter: ModCompatibilityFilter,
    ) {
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
            author: self.author.clone(),
        };
        if self.kind != ProjectKind::Mod {
            filters.loader = None;
        }
        filters
    }

    pub fn author(&self) -> Option<&AuthorFilter> {
        self.author.as_ref()
    }

    /// Shows projects of one author (or everyone's with `None`).
    pub fn set_author(&mut self, author: Option<AuthorFilter>) {
        if self.author != author {
            self.author = author;
            self.page = 0;
            self.invalidate_search();
        }
    }

    pub fn set_game_version_filter(&mut self, version: String) {
        self.game_version_filter = (!version.trim().is_empty()).then(|| version.trim().to_owned());
        self.page = 0;
        self.invalidate_search();
    }

    /// Picking the active category again turns the filter off.
    pub fn toggle_category(&mut self, category: ModCategory) {
        self.set_category(if self.category == category {
            ModCategory::All
        } else {
            category
        });
    }

    /// Picking the active source again turns the filter off.
    pub fn toggle_source(&mut self, source: catalog::ModSource) {
        self.set_source(if self.source == source {
            catalog::ModSource::All
        } else {
            source
        });
    }

    /// Picking the active loader again turns the filter off.
    pub fn toggle_loader_filter(&mut self, loader: Option<ModLoaderFilter>) {
        self.set_loader_filter(if self.loader_filter == loader {
            None
        } else {
            loader
        });
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
        self.author = None;
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
        self.project_info = None;
        self.info_loading = true;
        self.info_error = None;
        self.gallery_index = 0;
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
        self.project_info = None;
        self.info_loading = false;
        self.info_error = None;
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

    /// The loaded provider details of the selected project, if any.
    pub fn project_info(&self) -> Option<&catalog::ModProjectInfo> {
        self.project_info.as_ref()
    }

    pub fn info_loading(&self) -> bool {
        self.info_loading
    }

    pub fn info_error(&self) -> Option<&str> {
        self.info_error.as_deref()
    }

    /// Stores the result of one background project details request.
    pub fn finish_info_load(
        &mut self,
        request_id: u64,
        result: Result<catalog::ModProjectInfo, catalog::CatalogProviderError>,
    ) -> bool {
        if self.description_request_id != request_id || self.selected_mod.is_none() {
            return false;
        }

        self.info_loading = false;
        match result {
            Ok(info) => {
                self.project_info = Some(info);
                self.info_error = None;
            }
            Err(error) => {
                self.info_error = Some(error.message);
            }
        }
        true
    }

    /// The selected screenshot position of the project gallery.
    pub fn gallery_index(&self) -> usize {
        self.gallery_index
    }

    /// Moves the gallery to `index`, clamped to the loaded gallery.
    pub fn set_gallery_index(&mut self, index: usize, gallery_len: usize) {
        self.gallery_index = index.min(gallery_len.saturating_sub(1));
    }

    pub fn detail_tab(&self) -> ModDetailTab {
        self.detail_tab
    }

    pub fn set_detail_tab(&mut self, tab: ModDetailTab) {
        self.detail_tab = tab;
    }

    /// Mods installed into the given build for the build details page.
    pub fn build_mods(
        &self,
        profile: crate::features::instances::InstanceId,
    ) -> &[installed::InstalledModEntry] {
        self.build_mods
            .get(&profile)
            .map(|entries| entries.as_slice())
            .unwrap_or(&[])
    }

    pub fn set_build_mods(
        &mut self,
        profile: crate::features::instances::InstanceId,
        entries: Vec<installed::InstalledModEntry>,
    ) {
        self.build_mods.insert(profile, entries);
    }

    /// The description shown in the card and its tooltip.
    ///
    /// Returns the translation when it is ready, otherwise the original
    /// provider text. English never requests a translation.
    pub fn set_translate(&mut self, enabled: bool) {
        self.translate = enabled;
    }

    pub fn translates(&self) -> bool {
        self.translate
    }

    pub fn card_description(&self, project: &catalog::ModProject, locale: Locale) -> String {
        if locale != Locale::Ru || !self.translate {
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

pub fn mod_loader_filter(loader: crate::features::versions::LoaderChoice) -> ModLoaderFilter {
    match loader {
        crate::features::versions::LoaderChoice::Vanilla => ModLoaderFilter::Vanilla,
        crate::features::versions::LoaderChoice::Fabric => ModLoaderFilter::Fabric,
        crate::features::versions::LoaderChoice::Forge => ModLoaderFilter::Forge,
        crate::features::versions::LoaderChoice::NeoForge => ModLoaderFilter::NeoForge,
    }
}

/// Master switch for the CurseForge integration. Even when on, CurseForge is
/// used only if it is reachable (see `curseforge_api`).
pub const CURSEFORGE_ENABLED: bool = true;

/// Where CurseForge requests go. Release builds never carry the API key: they
/// talk to the OrbMC proxy (`infra/curseforge-proxy`), which adds the key on
/// the server. A developer's own key goes straight to the API.
#[derive(Clone)]
pub struct CurseForgeApi {
    base: String,
    key: Option<String>,
}

impl CurseForgeApi {
    /// `path` starts with `/v1/`, exactly as in the CurseForge API docs.
    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// The `x-api-key` header value; `None` behind the proxy.
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    pub fn authorize(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        match self.key() {
            Some(key) => request.header("x-api-key", key),
            None => request,
        }
    }
}

/// `CURSEFORGE_API_KEY` at run time talks to the API directly; otherwise the
/// proxy from `ORBMC_CURSEFORGE_PROXY` (run time, then baked in at build
/// time). Without either (or with CurseForge switched off) only Modrinth is
/// used.
pub fn curseforge_api() -> Option<CurseForgeApi> {
    if !CURSEFORGE_ENABLED {
        return None;
    }
    let present = |value: &String| !value.trim().is_empty();
    if let Some(key) = std::env::var("CURSEFORGE_API_KEY").ok().filter(present) {
        return Some(CurseForgeApi {
            base: "https://api.curseforge.com".to_owned(),
            key: Some(key.trim().to_owned()),
        });
    }
    let proxy = std::env::var("ORBMC_CURSEFORGE_PROXY")
        .ok()
        .or_else(|| option_env!("ORBMC_CURSEFORGE_PROXY").map(str::to_owned))
        .filter(present)?;
    Some(CurseForgeApi {
        base: proxy.trim().trim_end_matches('/').to_owned(),
        key: None,
    })
}
