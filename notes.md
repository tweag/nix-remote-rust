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
