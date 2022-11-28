use rust_nix_bazel::NixReadWrite;

fn main() {
    let mut rw = NixReadWrite {
        read: std::io::stdin(),
        write: std::io::stdout(),
    };

    rw.process_connection().unwrap_or_else(|e| {
        eprintln!("{e:?}");
    });
}
