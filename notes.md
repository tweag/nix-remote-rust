https://github.com/NixOS/nix/blob/master/src/libstore/remote-store.cc
https://github.com/bazelbuild/remote-apis/blob/main/build/bazel/remote/execution/v2/remote_execution.proto

## remote store
- works by ssh
- ssh in and call a binary
- communicates by stdin / stdout
- we write that binary
- nix-remote-build --custom-command <ours>
  - not actual flag

## protocol
- custom serialization
  - everything padded to n*64 bits
  - i64 (little-endian)
  - String (len: u64, data, padding) 
- wop*
  - ops for messages server -> client
  - some obsolete
  - correspond to functions on the client
  - lots of things depend on the version of the protocol
    - (at first) ignore this and implement latest version

## first steps
- capture commands & don't do much
- need to send something back
- forward to regular binary?
- another implementation of the same protocol is useful on its own

- enum for operations
- wire protocol

## bazel RE protocol

- on EnsurePath we could do substitution (using the blob cache). Why do we sometimes get AddToStore and sometimes EnsurePath?
- build an adaptation from nix store <-> bazel ca store
  it seems like nix calls AddToStore in dependency order, giving us the content each time, so we can compute the ca hashes
- encode a nix store path as an action in the action cache (action cache maps from the input hash of a build action to
  its output hashes)
- nix store queries can be turned into bazel actions

- using nix as a remote builder seems much easier. From a clean nix store, the ops go:
  - QueryValidPaths for the paths we want. Returns empty (when builders_use_substitutes is true, the builder does the downloading and returns the full set of paths)
  - AddMultipleToStore for adding everything
  - BuildDerivation build everything
  - QueryPathInfo


