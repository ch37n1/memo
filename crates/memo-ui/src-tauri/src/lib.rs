pub mod commands;

#[cfg(all(feature = "tauri-app", not(clippy), not(test)))]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            commands::health,
            commands::list_mounts,
            commands::create_mount,
            commands::remove_mount,
            commands::list_tokens,
            commands::create_token,
            commands::revoke_token,
            commands::query_audit,
            commands::browse_tree
        ])
        .run(tauri::generate_context!())
        .expect("error while running memo-ui");
}

#[cfg(any(not(feature = "tauri-app"), clippy, test))]
pub fn run() {}
