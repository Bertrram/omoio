pub mod archive;
pub mod covers;
mod backends;
mod big_picture;
mod commands;
mod controllers;
pub mod core;
mod hardware;
pub mod import;
mod pads;
mod figure_pictures;
mod portal_menu;
pub mod session;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(commands::InstallState::default())
        .manage(session::Session::default())
        // The game picture is a separate window sitting over ours, so it has to
        // be moved whenever ours is, and taken down when ours closes rather
        // than left running with nothing to sit on.
        .on_window_event(|window, event| {
            use tauri::{Manager, WindowEvent};
            let app = window.app_handle();
            let session = app.state::<session::Session>();
            match event {
                WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                    if let Some(main) = app.get_webview_window("main") {
                        session::place(&main, &session);
                    }
                }
                WindowEvent::Destroyed => {
                    session.stop();
                }
                _ => {}
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            #[cfg(desktop)]
            app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;
            big_picture::start(app.handle());
            figure_pictures::tidy(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_hardware_info,
            commands::get_rpcs3_version,
            commands::install_rpcs3,
            commands::cancel_rpcs3_install,
            commands::get_firmware_version,
            commands::install_firmware,
            commands::list_games,
            commands::import_game,
            commands::import_check,
            commands::game_warning,
            commands::launch_game,
            commands::launch_warning,
            commands::game_settings,
            commands::set_game_settings,
            commands::stop_game,
            commands::playing_game,
            commands::set_game_fullscreen,
            commands::list_sessions,
            commands::read_session_log,
            commands::session_prompt,
            commands::game_compatibility,
            commands::refresh_compatibility,
            commands::community_packs,
            commands::set_community_pack,
            commands::refresh_community,
            commands::cancel_community,
            commands::scan_folder,
            commands::game_updates,
            commands::install_update,
            commands::cancel_update,
            commands::game_saves,
            commands::back_up_saves,
            commands::restore_saves,
            commands::forget_backup,
            commands::get_account,
            commands::list_regions,
            commands::set_username,
            commands::set_region,
            commands::needs_setup,
            commands::finish_setup,
            commands::pending_updates,
            commands::catalogue,
            commands::add_to_library,
            commands::installed_packages,
            commands::install_package,
            commands::remove_package,
            commands::dropped_kind,
            commands::cancel_compatibility,
            commands::controller_view,
            commands::set_up_controller,
            commands::save_controller,
            commands::forget_controller,
            commands::set_covers,
            commands::set_rawg_key,
            commands::fetch_covers,
            commands::catalogue_cover,
            commands::emulator_versions,
            commands::install_cemu,
            commands::cancel_cemu_install,
            commands::install_dolphin,
            commands::cancel_dolphin_install,
            commands::cemu_keys,
            commands::add_cemu_keys,
            commands::look_at_own_cemu,
            commands::bring_own_cemu,
            commands::replace_with_own_save,
            commands::emulator_updates,
            commands::pad_input,
            commands::portal_figures,
            commands::portal_load,
            commands::portal_clear,
            commands::figure_pictures,
            commands::get_figure_pictures,
            commands::stop_figure_pictures,
            commands::figures,
            commands::villains,
            commands::add_figures,
            commands::delete_figure,
            commands::close_portal_menu,
            commands::portal_menu_family,
            commands::portal_game,
            commands::portal_made_figures,
            commands::pads_held,
            commands::figure_characters,
            commands::portal_create,
            commands::portal_button,
            commands::set_portal_button,
            commands::get_settings,
            commands::set_start_fullscreen,
            commands::set_keep_sessions,
            commands::get_places,
            commands::reveal_folder,
            commands::forget_all_games,
            commands::clear_session_logs,
            commands::remove_game,
            commands::get_games_folder,
            commands::set_games_folder,
            commands::import_archive,
            commands::cancel_import,
            commands::big_picture,
            commands::set_big_picture,
            commands::resume_game,
            commands::set_start_in_big_picture,
            commands::pads_connected,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
