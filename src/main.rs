use clap::Parser;
use rust_nix_bazel::{NixProxy, NixReadWrite, NixStoreRead, NixStoreWrite};

#[derive(Parser, Debug)]
#[command()]
struct Args {
    /// Whether to proxy to nix
    #[arg(long)]
    proxy_to_nix: bool,
}

fn main() {
    let args = Args::parse();

    let proxy = NixProxy::new();

    let mut rw = NixReadWrite {
        read: NixStoreRead {
            inner: std::io::stdin(),
        },
        write: NixStoreWrite {
            inner: std::io::stdout(),
        },
        proxy,
    };

    rw.process_connection(true).unwrap_or_else(|e| {
        eprintln!("{e:?}");
    });
}
