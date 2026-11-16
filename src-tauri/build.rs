fn main() {
  // 图标变更必须触发资源重编，否则 exe 里会一直是旧图标
  println!("cargo:rerun-if-changed=icons");
  println!("cargo:rerun-if-changed=icons/icon.ico");
  println!("cargo:rerun-if-changed=tauri.conf.json");
  tauri_build::build()
}
