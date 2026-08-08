mod diagnostics;
mod features;
mod paths;
mod position;
mod server;
mod sync;

use std::error::Error;

use lsp_server::Connection;
use lsp_types as lsp;

use server::Server;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let (connection, io_threads) = Connection::stdio();
    let capabilities = serde_json::to_value(features::capabilities())?;
    let initialization = connection.initialize(capabilities)?;
    let _: lsp::InitializeParams = serde_json::from_value(initialization)?;

    let mut server = Server::new(connection);
    server.run()?;
    drop(server);
    io_threads.join()?;
    Ok(())
}
