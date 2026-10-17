/* 切换悬浮窗显示/隐藏；首次显示时定位到主显示器右上角 */
fn toggle_pip(handle: &tauri::AppHandle) {
  use tauri::Manager;
  if let Some(w) = handle.get_webview_window("pip") {
    if w.is_visible().unwrap_or(false) {
      let _ = w.hide();
    } else {
      if let Ok(Some(monitor)) = w.current_monitor() {
        let mpos = monitor.position();
        let msize = monitor.size();
        if let Ok(size) = w.outer_size() {
          let x = mpos.x + msize.width as i32 - size.width as i32 - 16;
          let _ = w.set_position(tauri::PhysicalPosition::new(x.max(mpos.x), mpos.y + 16));
        }
      }
      let _ = w.show();
    }
  }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  use tauri::Listener;
  tauri::Builder::default()
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }

      /* 前端通过事件请求切换悬浮窗（emit('toggle-pip')），无需自定义命令权限 */
      let handle = app.handle().clone();
      app.listen("toggle-pip", move |_| {
        toggle_pip(&handle);
      });

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while building tauri application");
}
