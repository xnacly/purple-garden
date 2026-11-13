mod server;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    server::run()
}
