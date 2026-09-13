#![forbid(unsafe_code)]

fn main() {
    if let Err(error) = mactype_service_host::validate_host_arguments(std::env::args().skip(1)) {
        eprintln!("{error}");
        std::process::exit(2);
    }
    if let Err(error) = mactype_service_host::run_service_process() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
