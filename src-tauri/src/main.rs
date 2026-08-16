#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some((job_id, parent_pid)) = relaypulse_lib::intelligence_ladder::scraper_sidecar_args()
    {
        std::process::exit(relaypulse_lib::intelligence_ladder::run_scraper_sidecar(
            job_id, parent_pid,
        ));
    }
    relaypulse_lib::run();
}
