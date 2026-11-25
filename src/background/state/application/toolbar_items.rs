use std::sync::LazyLock;

use seelen_core::{
    resource::WidgetId,
    state::{ToolbarItem, ToolbarItem2, ToolbarState},
};

use crate::{
    error::{Result, ResultLogExt},
    utils::{atomic_write_file, constants::SEELEN_COMMON, lock_free::TracedMutex},
};

pub static TOOLBAR_ITEMS_MANAGER: LazyLock<ToolbarItemsManager> = LazyLock::new(|| {
    let manager = ToolbarItemsManager {
        items: TracedMutex::new(ToolbarState::default()),
    };
    manager.load().log_error();
    manager
});

pub struct ToolbarItemsManager {
    items: TracedMutex<ToolbarState>,
}

impl ToolbarItemsManager {
    pub fn get(&self) -> ToolbarState {
        self.items.lock().clone()
    }

    pub fn write(&self, mut items: ToolbarState) -> Result<()> {
        items.sanitize();
        let path = SEELEN_COMMON
            .widget_data_dir(&WidgetId::known_toolbar())
            .join("state.yml");
        // Hold the lock while writing so concurrent saves (one webview per
        // monitor) are serialized instead of racing on the same file.
        let mut guard = self.items.lock();
        atomic_write_file(&path, serde_yaml::to_string(&items)?.as_bytes())?;
        *guard = items;
        Ok(())
    }

    pub fn load(&self) -> Result<()> {
        let path = SEELEN_COMMON
            .widget_data_dir(&WidgetId::known_toolbar())
            .join("state.yml");

        let items = if path.exists() {
            let mut items: ToolbarState = serde_yaml::from_str(&std::fs::read_to_string(&path)?)?;
            items.sanitize();
            // one-time change of a layout saved by earlier builds (can be changed back after)
            let marker = path.with_file_name("edited-migrated-layout");
            if !marker.exists() {
                apply_edited_layout(&mut items);
                atomic_write_file(&path, serde_yaml::to_string(&items)?.as_bytes())?;
                atomic_write_file(&marker, b"done")?;
            }
            items
        } else {
            let mut items = initial_items();
            items.sanitize();
            atomic_write_file(&path, serde_yaml::to_string(&items)?.as_bytes())?;
            items
        };

        *self.items.lock() = items;
        Ok(())
    }
}

fn initial_items() -> ToolbarState {
    ToolbarState {
        left: vec![
            ToolbarItem2::Plugin("@seelen/tb-user-menu".into()),
            ToolbarItem2::Inline(Box::new(ToolbarItem {
                template: "return \"|\"".into(),
                ..Default::default()
            })),
            ToolbarItem2::Plugin("@default/focused-app".into()),
        ],
        center: vec![ToolbarItem2::Plugin("@seelen/tb-calendar-popup".into())],
        right: vec![
            ToolbarItem2::Plugin("@default/cpu-usage".into()),
            ToolbarItem2::Plugin("@default/gpu-usage".into()),
            ToolbarItem2::Plugin("@default/memory-usage".into()),
            ToolbarItem2::Plugin("@default/network-usage".into()),
            ToolbarItem2::Plugin("@seelen/tb-system-tray".into()),
            ToolbarItem2::Plugin("@seelen/tb-keyboard-selector".into()),
            ToolbarItem2::Plugin("@seelen/tb-bluetooth-popup".into()),
            ToolbarItem2::Plugin("@seelen/tb-network-popup".into()),
            ToolbarItem2::Plugin("@seelen/tb-media-popup".into()),
            ToolbarItem2::Plugin("@default/power".into()),
            ToolbarItem2::Plugin("@seelen/tb-notifications".into()),
            ToolbarItem2::Plugin("@seelen/tb-quick-settings".into()),
        ],
        ..Default::default()
    }
}

/// Only the focused app's icon and name on the left (not its window title, which can be long),
/// and live CPU, GPU, memory and network usage on the right.
fn apply_edited_layout(items: &mut ToolbarState) {
    let is_title = |item: &ToolbarItem2| match item {
        ToolbarItem2::Plugin(id) => id.to_string() == "@default/focused-app-title",
        // the "-" shown between the app name and its title
        ToolbarItem2::Inline(inline) => inline.template.contains("focusedApp.title"),
    };
    for side in [&mut items.left, &mut items.center, &mut items.right] {
        side.retain(|item| !is_title(item));
    }

    let has = |items: &ToolbarState, plugin: &str| {
        [&items.left, &items.center, &items.right]
            .iter()
            .flat_map(|side| side.iter())
            .any(|item| matches!(item, ToolbarItem2::Plugin(id) if id.to_string() == plugin))
    };
    let stats = [
        "@default/cpu-usage",
        "@default/gpu-usage",
        "@default/memory-usage",
        "@default/network-usage",
    ];
    let missing: Vec<ToolbarItem2> = stats
        .iter()
        .filter(|plugin| !has(items, plugin))
        .map(|plugin| ToolbarItem2::Plugin((*plugin).into()))
        .collect();
    items.right.splice(0..0, missing);
}
