# nix-remote

This is a re-implementation of Nix's remote build protocol. The main objectives are:

 - to express the protocol declaratively, as much as possible;
 - to document the protocol better than it has been so far;
 - to provide a library for tools making use of Nix remote builds.

I believe that we have implemented all worker ops used in the
current version of the nix protocol. (Nix itself supports more ops, but only for
backwards-compatibility.)

## Example: daemon proxy

The library has been used to implement a nix remote proxy as an example, which
forwards commands to a real `nix-daemon`, while inspecting the commands and
the responses.

To build the example and use `nix` to connect to it as remote, run:

```bash
cargo build --example proxy
nix store info --store "ssh-ng://localhost?remote-program=${PWD}/target/debug/examples/proxy"
```
