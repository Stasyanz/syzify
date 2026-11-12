//! The app menu: Tauri's default, but on macOS with a Quit of our own.
//!
//! The default Quit item sends `terminate:`, which ends the process without
//! a `RunEvent::ExitRequested` — nothing could finish an indoor ride under
//! it, and the trainer would keep its brake on. Our item (Cmd+Q too) asks
//! for the exit through `AppHandle::exit`, which does raise
//! `ExitRequested`, so a ride is finished first (`lib.rs`).
//!
//! Quit from the Dock, a log-out and a restart of the Mac still go through
//! `terminate:`: there the ride's journal survives (offered at the next
//! start) and the trainer is not let go — an accepted limit.

/// The id of our Quit item.
pub const QUIT_ID: &str = "syzify-quit";

/// Whether a menu event is our Quit.
pub fn is_quit(id: &str) -> bool {
    id == QUIT_ID
}

/// The default menu with the app submenu's Quit replaced by ours.
#[cfg(target_os = "macos")]
pub fn app_menu<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
    let menu = Menu::default(app)?;
    let pkg = app.package_info();
    let config = app.config();
    let about = AboutMetadata {
        name: Some(pkg.name.clone()),
        version: Some(pkg.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config.bundle.publisher.clone().map(|p| vec![p]),
        ..Default::default()
    };
    let quit = MenuItem::with_id(app, QUIT_ID, format!("Quit {}", pkg.name), true, Some("CmdOrCtrl+Q"))?;
    let app_submenu = Submenu::with_items(
        app,
        pkg.name.clone(),
        true,
        &[
            &PredefinedMenuItem::about(app, None, Some(about))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    // The default's first submenu is the app's on macOS.
    menu.remove_at(0)?;
    menu.prepend(&app_submenu)?;
    Ok(menu)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn our_quit_is_known_by_its_id() {
        assert!(is_quit(QUIT_ID));
        assert!(!is_quit("quit"));
    }
    // `app_menu` is checked by hand: AppKit builds menus on the main thread
    // only, and tests run on others.
}
