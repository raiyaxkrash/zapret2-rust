fn main() {
    // winws2 и драйвер WinDivert требуют прав администратора,
    // поэтому приложение всегда запускается с повышением прав (UAC).
    let windows = tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest"));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri-build");
}
