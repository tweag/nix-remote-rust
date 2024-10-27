use nix_remote::NixProxy;

fn main() {
    let Ok(target_store) = std::env::var("NIX_COPY_PATHS_TO") else {
        eprintln!("Must specify target store in NIX_COPY_PATHS_TO environment variable");
        std::process::exit(1)
    };
    let mut proxy = NixProxy::new(std::io::stdin(), std::io::stdout(), &target_store);

    proxy.process_connection().unwrap_or_else(|e| {
        eprintln!("{e:?}");
    });
}
