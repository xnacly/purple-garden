mod analysis;
mod code_action;
mod completion;
mod diagnostic;
mod server;
mod source;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    server::run()
}
