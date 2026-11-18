fn main() {
  // 图标变更必须触发资源重编，否则 exe 里会一直是旧图标
  println!("cargo:rerun-if-changed=icons");
  println!("cargo:rerun-if-changed=icons/icon.ico");
  println!("cargo:rerun-if-changed=tauri.conf.json");
  // 前端文件是嵌进 exe 的（frontendDist），改完必须重编才会生效
  println!("cargo:rerun-if-changed=../pip.html");
  println!("cargo:rerun-if-changed=../index.html");
  tauri_build::build()
}
