#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }

      // 悬浮窗：定位到主显示器右上角后再显示（避免闪现默认位置）
      use tauri::Manager;
      if let Some(window) = app.get_webview_window("pip") {
        if let Ok(Some(monitor)) = window.current_monitor() {
          let mpos = monitor.position();
          let msize = monitor.size();
          if let Ok(size) = window.outer_size() {
            let x = mpos.x + msize.width as i32 - size.width as i32 - 16;
            let y = mpos.y + 16;
            let _ = window.set_position(tauri::PhysicalPosition::new(x.max(mpos.x), y));
          }
        }
        let _ = window.show();
      }

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while building tauri application");
}
