pub(crate) fn run() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 4 || args[0] != "--offline-source" || args[2] != "--destination" {
        eprintln!("Usage: relaypulse-data-transfer --offline-source STOPPED_SOURCE_DIR --destination NEW_DIR");
        std::process::exit(2);
    }
    match super::convert_offline(
        std::path::Path::new(&args[1]),
        std::path::Path::new(&args[3]),
    ) {
        Ok(report) => println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("verification serializes")
        ),
        Err(_) => {
            eprintln!("Conversion refused or verification failed. Source retained; destination must not be activated without a successful verification report.");
            std::process::exit(1);
        }
    }
}
