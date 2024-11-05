use nix_remote::worker_op::StreamingRecv;
use nix_remote::worker_op::WorkerOp;
use nix_remote::{nix_client::NixDaemonClient, nix_daemon_proxy::NixDaemonProxy, stderr::Msg};

macro_rules! for_each_op {
    ($macro_name:ident !) => {
        $macro_name!(
            IsValidPath,
            QueryReferrers,
            AddToStore,
            BuildPaths,
            EnsurePath,
            AddTempRoot,
            FindRoots,
            SetOptions,
            CollectGarbage,
            QueryAllValidPaths,
            QueryPathInfo,
            QueryPathFromHashPart,
            QueryValidPaths,
            QuerySubstitutablePaths,
            QueryValidDerivers,
            OptimiseStore,
            VerifyStore,
            BuildDerivation,
            AddSignatures,
            NarFromPath,
            AddToStoreNar,
            QueryMissing,
            QueryDerivationOutputMap,
            RegisterDrvOutput,
            QueryRealisation,
            AddMultipleToStore,
            AddBuildLog,
            BuildPathsWithResults
        )
    };
}

fn main() {
    let mut daemon = NixDaemonProxy::new(std::io::stdin(), std::io::stdout()).unwrap();

    let mut child = std::process::Command::new("nix-daemon")
        .arg("--stdio")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    let mut client =
        NixDaemonClient::new(child.stdout.take().unwrap(), child.stdin.take().unwrap()).unwrap();

    loop {
        match &daemon.receive_next_op_from_client() {
            Err(e) => {
                eprintln!("{e:?}");
            }
            Ok(worker_op) => {
                client.send_worker_op_to_daemon(worker_op).unwrap();

                if worker_op.requires_streaming() {
                    const BUF_SIZE: usize = 4096;
                    let mut buff = vec![0; BUF_SIZE];

                    loop {
                        let mut length_of_stream = daemon.streaming_length().unwrap();
                        client.streaming_write_len(length_of_stream as u64).unwrap();

                        if length_of_stream == 0 {
                            break;
                        }
                        while length_of_stream > 0 {
                            let chunk_len = length_of_stream.min(BUF_SIZE);
                            daemon.get_stream(chunk_len, &mut buff).unwrap();
                            client.streaming_write_buff(&buff, chunk_len).unwrap();
                            length_of_stream -= chunk_len;
                        }
                    }
                    client.flush().unwrap();
                }
                // wait for proxy response
                loop {
                    let error_message_from_builder = client.read_error_msg().unwrap();
                    daemon
                        .send_error_to_client(&error_message_from_builder)
                        .unwrap();
                    // forward message to client via our daemon
                    if error_message_from_builder == Msg::Last(()) {
                        break;
                    }
                }
                // forward response from builder to client
                macro_rules! respond {
                    ($($name:ident),*) => {
                        #[allow(unreachable_patterns)]
                        match worker_op {
                            // Special case for NarFromPath because the response could be large
                            // and needs to be streamed instead of read into memory.
                            WorkerOp::NarFromPath(_inner, _resp) => {
                                nix_remote::nar::stream(client.reader(), daemon.writer()).unwrap();
                            }
                            $(WorkerOp::$name(_inner, resp) => {
                                let reply = client.read_build_response_from_daemon(resp).unwrap();
                                // eprintln!("Forwarding build response to client {reply:?}");
                                daemon.write_build_response_to_client(&reply).unwrap();
                            },)*
                        }
                    };
                }
                for_each_op!(respond!);

                daemon.flush_tx_to_client().unwrap();
            }
        }
    }
}
