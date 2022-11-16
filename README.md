# rust-nix-bazel

This is the rust channel's little project: a tool for proxying nix remote builds to bazel.

- TODO: come up with a catchy name
- ask in #rust if you want to help out!

## Usage

To build the project and use `nix` to connect to it as remote, run:

```bash
cargo build
nix store ping --store 'ssh-ng://localhost?remote-program=/PATH/TO/rust-nix-bazel/target/debug/rust-nix-bazel'
```
