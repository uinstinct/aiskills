mod catalog;
mod cli;
mod harness;
mod http;
mod installer;
mod state;
mod tui;
mod update;

fn main() {
    if let Err(e) = tui::run() {
        eprintln!("instinctagents: {e}");
        std::process::exit(1);
    }
}
