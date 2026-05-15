use clap::Parser;
use instinctagents::{cli, tui};

fn main() {
    let args = cli::Cli::parse();

    if args.non_interactive {
        let cwd = match std::env::current_dir() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("instinctagents: {e}");
                std::process::exit(1);
            }
        };
        match cli::run_non_interactive(&args, &cwd) {
            Ok(outcome) => println!("{outcome}"),
            Err(e) => {
                eprintln!("instinctagents: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    if let Err(e) = tui::run(args.verbose) {
        eprintln!("instinctagents: {e}");
        std::process::exit(1);
    }
}
