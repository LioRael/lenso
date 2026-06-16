#[allow(clippy::wildcard_imports)]
use super::*;

static STORY_DISPLAY: OnceLock<RwLock<InstalledCatalog<StoryDisplayDescriptor>>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatalogMode {
    Default,
    Runtime,
}

#[derive(Debug)]
struct InstalledCatalog<T> {
    mode: CatalogMode,
    items: Vec<T>,
}

impl<T> Default for InstalledCatalog<T> {
    fn default() -> Self {
        Self {
            mode: CatalogMode::Default,
            items: Vec::new(),
        }
    }
}

/// Install the aggregated Story display catalog from loaded module metadata.
pub fn install_story_display(catalog: Vec<StoryDisplayDescriptor>) {
    install_catalog(catalog, CatalogMode::Runtime);
}

/// Install context-free default Story display metadata for router/OpenAPI setup.
pub fn install_default_story_display(catalog: Vec<StoryDisplayDescriptor>) {
    install_catalog(catalog, CatalogMode::Default);
}

fn install_catalog(items: Vec<StoryDisplayDescriptor>, mode: CatalogMode) {
    let catalog = STORY_DISPLAY.get_or_init(|| RwLock::new(InstalledCatalog::default()));
    let mut catalog = catalog.write().expect("story catalog lock poisoned");
    if mode == CatalogMode::Default && catalog.mode == CatalogMode::Runtime {
        return;
    }
    *catalog = InstalledCatalog { mode, items };
}

pub(super) fn story_display_catalog() -> Vec<StoryDisplayDescriptor> {
    STORY_DISPLAY
        .get()
        .map(|catalog| {
            catalog
                .read()
                .expect("story catalog lock poisoned")
                .items
                .clone()
        })
        .unwrap_or_default()
}

#[doc(hidden)]
#[cfg(debug_assertions)]
pub fn story_display_catalog_snapshot() -> Vec<StoryDisplayDescriptor> {
    story_display_catalog()
}

#[doc(hidden)]
#[cfg(debug_assertions)]
pub fn reset_catalogs_for_test() {
    if let Some(catalog) = STORY_DISPLAY.get() {
        *catalog.write().expect("story catalog lock poisoned") = InstalledCatalog::default();
    }
}
