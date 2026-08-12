use std::process::Command;

fn main() {
    println!("🚀 Translatore Web Control Center v3.0");

    // Ensure pt-control backend daemon is active
    let _ = Command::new("pt-control").spawn();

    let url = "http://127.0.0.1:9090";
    println!("Opening Control Dashboard: {url}");

    // Open URL in default browser
    #[cfg(target_os = "linux")]
    let _ = Command::new("xdg-open").arg(url).spawn();

    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(url).spawn();

    #[cfg(target_os = "windows")]
    let _ = Command::new("cmd").args(["/C", "start", url]).spawn();
}
