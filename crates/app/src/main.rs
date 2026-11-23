//! Thread Vision: measure diameters and threads with the backlit telecentric camera.

mod app;
mod log;
mod settings;
mod worker;

use std::path::{Path, PathBuf};

fn main() -> eframe::Result {
    // Relative paths (captures/calibration.txt, measurements.csv) follow the working directory,
    // which is the exe's own folder when it is double-clicked. Unless the launch folder already
    // holds captures/ (a shortcut's "Start in"), work from THREAD_VISION_DATA (set by the macOS
    // app bundle, whose exe lives outside the repository) or the exe's data folder instead.
    let configured = std::env::var_os(DATA_ENV).map(PathBuf::from);
    let exe = std::env::current_exe().ok();
    let exe_dir = exe.as_deref().and_then(Path::parent);
    if let Some(root) = working_root(
        Path::new("captures").is_dir(),
        configured.as_deref(),
        exe_dir,
    ) {
        let _ = std::env::set_current_dir(root);
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1500.0, 900.0])
            .with_title("Thread Vision"),
        ..Default::default()
    };
    eframe::run_native(
        "Thread Vision",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

/// Folder holding `captures/`, for an exe that does not live beside its data.
const DATA_ENV: &str = "THREAD_VISION_DATA";

/// Where to work from: `None` keeps the launch folder (it already holds `captures/`); otherwise
/// the configured folder if it holds `captures/`, else the exe's data folder.
fn working_root(
    launched_in_data: bool,
    configured: Option<&Path>,
    exe_dir: Option<&Path>,
) -> Option<PathBuf> {
    if launched_in_data {
        return None;
    }
    configured
        .filter(|d| d.join("captures").is_dir())
        .or_else(|| exe_dir.and_then(data_root))
        .map(Path::to_path_buf)
}

/// The nearest of the exe's folder and its two parents that holds `captures/`: the repository
/// for a built `target\release\app.exe`, or the exe's own folder when shipped beside its data.
/// `None` leaves the working directory as launched.
fn data_root(exe_dir: &Path) -> Option<&Path> {
    exe_dir
        .ancestors()
        .take(3)
        .find(|d| d.join("captures").is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_tree(name: &str, dirs: &[&str]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("tv_root_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for d in dirs {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        root
    }

    #[test]
    fn built_exe_uses_the_repository_captures() {
        let root = temp_tree("repo", &["captures", "target/release"]);
        let exe_dir = root.join("target").join("release");
        assert_eq!(data_root(&exe_dir), Some(root.as_path()));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn shipped_exe_prefers_its_own_captures() {
        let root = temp_tree("shipped", &["captures", "app/captures"]);
        let exe_dir = root.join("app");
        assert_eq!(data_root(&exe_dir), Some(exe_dir.as_path()));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn app_bundle_exe_uses_the_configured_repository() {
        let root = temp_tree(
            "bundle",
            &["repo/captures", "Thread Vision.app/Contents/MacOS"],
        );
        let repo = root.join("repo");
        let exe_dir = root.join("Thread Vision.app/Contents/MacOS");
        assert_eq!(
            working_root(false, Some(&repo), Some(&exe_dir)),
            Some(repo.clone())
        );
        // a launch folder that already holds captures/ wins over everything
        assert_eq!(working_root(true, Some(&repo), Some(&exe_dir)), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn configured_folder_without_captures_falls_back_to_the_exe() {
        let root = temp_tree("fallback", &["captures", "target/release", "empty"]);
        let exe_dir = root.join("target").join("release");
        assert_eq!(
            working_root(false, Some(&root.join("empty")), Some(&exe_dir)),
            Some(root.clone())
        );
        assert_eq!(working_root(false, None, None), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn no_captures_nearby_keeps_the_launch_folder() {
        let root = temp_tree("none", &["a/captures", "a/b/c/d"]);
        // a/captures is three folders above d; only d and its two parents are searched.
        assert_eq!(
            data_root(&root.join("a").join("b").join("c").join("d")),
            None
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
