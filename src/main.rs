use rust_nix_bazel::NixReadWrite;

fn main() {
    let mut rw = NixReadWrite {
        read: std::io::stdin(),
        write: std::io::stdout(),
    };
    rw.init_connection().unwrap();
}
