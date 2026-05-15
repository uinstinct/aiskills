mod catalog;
mod cli;
mod harness;
mod http;
mod installer;
mod state;
mod tui;
mod update;

fn main() {
    println!("instinctagents v{}", env!("CARGO_PKG_VERSION"));
}
