mod ball;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  use tauri::{Listener, Manager};
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
        ball::toggle(&handle);
      });

      /* pip 窗口移动 → 拖动结束 / 吸附判定（我们自己的动画移动会被 quiet_until 过滤） */
      if let Some(w) = app.get_webview_window("pip") {
        let h = app.handle().clone();
        w.on_window_event(move |e| {
          if matches!(e, tauri::WindowEvent::Moved(_)) {
            ball::on_moved(&h);
          }
        });
      }

      ball::init(app.handle().clone());

      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      ball::ball_state,
      ball::ball_drag,
      ball::ball_menu,
      ball::ball_action
    ])
    .run(tauri::generate_context!())
    .expect("error while building tauri application");
}
