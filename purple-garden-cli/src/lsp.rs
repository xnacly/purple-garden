mod analysis;
mod code_action;
mod collect;
mod completion;
mod definition;
mod diagnostic;
mod hover;
mod server;
mod source;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    server::run()
}
