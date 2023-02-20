use clap::Parser;
use rust_nix_bazel::{NixProxy, NixReadWrite};

#[derive(Parser, Debug)]
#[command()]
struct Args {
    /// Whether to proxy to nix
    #[arg(long)]
    proxy_to_nix: bool,
}

fn main() {
    let args = Args::parse();

    // let proxy = NixProxy::new();

    let mut rw = NixReadWrite {
        read: std::io::stdin(),
        write: std::io::stdout(),
        // proxy: todo!(),
    };

    rw.process_connection(true).unwrap_or_else(|e| {
        eprintln!("{e:?}");
    });
}
