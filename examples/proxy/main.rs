mod proxy;
use proxy::NixProxy;

fn main() {
    let mut proxy = NixProxy::new(std::io::stdin(), std::io::stdout());

    proxy.process_connection().unwrap_or_else(|e| {
        eprintln!("{e:?}");
    });
}
