fn main() {
    let args: Vec<String> = std::env::args().collect();
    std::process::exit(osdoctor::cli::run(&args));
}
