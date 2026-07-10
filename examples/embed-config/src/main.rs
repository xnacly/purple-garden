#[path = "config.rs"]
mod config_pkg;

use purple_garden::Pg;

fn main() {
    let input = include_bytes!("../config.garden");

    let mut program = Pg::new()
        .with_lib(&config_pkg::config::PACKAGE)
        .compile(input)
        .expect("config script should compile");

    let summary = program
        .run_take::<String>()
        .expect("config script should run");

    println!("{summary}");
}
