use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    App, Manager,
};

const OPEN_SETTINGS_ID: &str = "open-settings";
const EXIT_ID: &str = "exit";

pub(crate) fn setup(app: &mut App) -> tauri::Result<()> {
    let open_settings =
        MenuItem::with_id(app, OPEN_SETTINGS_ID, "Open Settings", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, EXIT_ID, "Exit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_settings, &exit])?;

    TrayIconBuilder::new()
        .tooltip("HanYeongKey")
        .icon(tray_icon())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            OPEN_SETTINGS_ID => show_settings(app),
            EXIT_ID => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

fn show_settings(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(crate::settings::WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn tray_icon() -> Image<'static> {
    const SIZE: u32 = 32;
    let mut rgba = vec![0_u8; (SIZE * SIZE * 4) as usize];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let index = ((y * SIZE + x) * 4) as usize;
            let inside = (4..28).contains(&x) && (4..28).contains(&y);
            if inside {
                rgba[index] = 37;
                rgba[index + 1] = 99;
                rgba[index + 2] = 235;
                rgba[index + 3] = 255;
            }
        }
    }

    Image::new_owned(rgba, SIZE, SIZE)
}
