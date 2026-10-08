#![cfg_attr(windows, windows_subsystem = "windows")]
fn main() {
    let args = match dji4g_updater::session::Arguments::parse(std::env::args_os().skip(1)) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    #[cfg(windows)]
    let result = dji4g_updater::session::run(&args);
    #[cfg(not(windows))]
    let result: Result<(), dji4g_updater::UpdateError> =
        Err(dji4g_updater::UpdateError("PLATFORM: Windows only".into()));
    if let Err(error) = result {
        let detail: String = format!("UPDATE_FAILED\n{error}")
            .chars()
            .take(2048)
            .collect();
        // Writes only into a checked session and never overwrites an existing file.
        let _ = dji4g_updater::session::write_marker(&args.work_dir, "error", &detail);
        eprintln!("{detail}");
        std::process::exit(1);
    }
}
