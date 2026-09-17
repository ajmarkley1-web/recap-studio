pub mod analyze;
pub mod commands;
pub mod compliance;
pub mod error;
pub mod events;
pub mod ingest;
pub mod model;
pub mod narrator;
pub mod panels;
pub mod project;
pub mod prompts;
pub mod providers;
pub mod script;
pub mod settings;
pub mod tts;
pub mod util;
pub mod video;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "recap_studio_lib=info,warn".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let state = commands::build_state(app.handle());
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // app
            commands::app_info,
            // settings
            commands::get_settings,
            commands::save_settings,
            commands::set_provider_key,
            commands::set_provider_base_url,
            commands::set_tts_key,
            commands::set_model_choice,
            commands::refresh_models,
            commands::cached_models,
            commands::test_provider,
            // projects
            commands::list_projects,
            commands::create_project,
            commands::open_project,
            commands::delete_project,
            commands::update_project,
            // ingest
            commands::stage_pdf_page,
            commands::ingest_pdf,
            commands::ingest_paths,
            // analyze
            commands::analyze_project,
            // scripts
            commands::narrator_prompt,
            commands::generate_script,
            commands::recheck_compliance,
            commands::auto_fix_script,
            commands::update_script_text,
            // narration
            commands::list_voices,
            commands::list_tts_models,
            commands::preview_voice,
            commands::narrate_script,
            // video
            commands::ffmpeg_status,
            commands::render_video,
            // export
            commands::export_script,
            commands::export_bible,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Recap Studio");
}
