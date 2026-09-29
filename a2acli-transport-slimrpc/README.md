# a2acli-transport-slimrpc

SLIMRPC transport plugin for the [Go A2A CLI](https://github.com/a2aproject/a2a-cli).
The crate builds the `a2a-transport-slimrpc` executable. Install it on `PATH` to
send A2A requests over SLIMRPC from the Go CLI's `a2a` command.

This plugin is separate from the Rust [`a2acli`](../a2acli/README.md) executable
in this workspace. It wraps [`a2a-slimrpc`](../a2a-slimrpc/README.md) using the
Go CLI's [transport-plugin contract](https://github.com/a2aproject/a2a-cli/blob/main/docs/transport-plugins.md).

## What It Provides

```text
a2a (Go CLI) -> local TLS gRPC proxy (plugin) -> SLIM gateway -> SLIMRPC agent
```

The CLI discovers the plugin with its `info` subcommand, then launches
`serve --endpoint <target>` when needed. The plugin reads its YAML configuration,
connects to the gateway, and gives the CLI a loopback address, an ephemeral TLS
certificate, and a per-launch authentication token. It forwards A2A calls to
the target agent and stops when the CLI closes its stdin. The local token is
checked by the proxy and removed before forwarding requests upstream.

## Build And Discover

Install a current stable Rust toolchain and the
[Go CLI](https://github.com/a2aproject/a2a-cli#installation) (`a2a`). The local
example below uses Go CLI v0.3.0. Native Rust dependencies also need a C/C++
toolchain and CMake. Docker is needed only for the example gateway.

Obtain the workspace if you do not already have a checkout:

```sh
git clone https://github.com/a2aproject/a2a-rs.git
cd a2a-rs
```

From the workspace root:

```sh
cargo build -p a2acli-transport-slimrpc --release --locked
export PATH="$PWD/target/release:$PATH"
a2a-transport-slimrpc info
a2a transport list
```

The transport list should include `slimrpc`. Its reported `GRPC` binding is the
local CLI-to-plugin connection; requests to the agent use SLIMRPC. Discovery
does not require a configuration file or running gateway and does not test
connectivity. Keep this shell open for the send commands below.

For a persistent source installation, use
`cargo install --path a2acli-transport-slimrpc --locked` and ensure Cargo's
binary directory is on `PATH`.

## Configuration

Set `A2A_SLIMRPC_PLUGIN_CONFIG` to the path of a YAML file. It is a **file path**,
not inline YAML. The file has two required sections:

```yaml
client:
  endpoint: "http://127.0.0.1:46357"
  tls:
    insecure: true
app:
  name: "org/demo/cli"
  identity_provider:
    type: shared_secret
    id: "demo-client"
    data: "slimrpc-local-demo-secret-at-least-32-bytes"
  identity_verifier:
    type: shared_secret
    id: "demo-client"
    data: "slimrpc-local-demo-secret-at-least-32-bytes"
```

| Field | Meaning |
| --- | --- |
| `client.endpoint` | Gateway address. Use `http://host:port` for the local plaintext gateway, not `grpc://`. |
| `client.tls.insecure` | `true` disables TLS on the gateway connection for this local demo. This does not disable TLS on the CLI's loopback proxy. |
| `app.name` | The plugin's own SLIM identity, as `org/namespace/name`, without a URL scheme. |
| `app.identity_provider` | How the plugin authenticates its outgoing SLIM messages. |
| `app.identity_verifier` | How the plugin verifies incoming SLIM messages. |

`client` is a SLIM
[`ClientConfig`](https://docs.rs/agntcy-slim-config/0.16.2/slim_config/client/struct.ClientConfig.html),
including gateway authentication, TLS, timeouts, and retry settings.
The identity fields use SLIM's
[`IdentityProviderConfig`](https://docs.rs/agntcy-slim-config/0.16.2/slim_config/auth/identity/enum.IdentityProviderConfig.html)
and
[`IdentityVerifierConfig`](https://docs.rs/agntcy-slim-config/0.16.2/slim_config/auth/identity/enum.IdentityVerifierConfig.html).
The `shared_secret` example requires a secret of at least 32 bytes; both peers
must use the same secret. These public demo credentials and plaintext gateway
settings are for local testing only. Configure gateway TLS and appropriate
identity credentials for your deployment.

The CLI's `--endpoint` names the **remote agent**, not the gateway and not
`app.name`. Accepted target forms are `org/demo/echo`, `slim://org/demo/echo`,
and `slimrpc://org/demo/echo`.

## Local End-To-End Example

This example requires the gateway connection fixes in
[#312](https://github.com/a2aproject/a2a-rs/pull/312). Ensure those fixes are
present in your checkout when building the plugin.

Run the following commands from the workspace root. Use three terminals:
one for the gateway, one for the echo agent, and the build shell for the CLI.
The example uses the published SLIM gateway image `2.3.0` with this workspace's
locked dependencies.

### 1. Start The Gateway

The checked-in [gateway configuration](examples/slim-gateway.yaml) starts a
plaintext listener inside the container. Publish it only on the host's loopback
interface:

```sh
docker run --rm --name a2a-slimrpc-demo \
  -p 127.0.0.1:46357:46357 \
  -v "$PWD/a2acli-transport-slimrpc/examples/slim-gateway.yaml:/config.yaml:ro" \
  ghcr.io/agntcy/slim:2.3.0 /slim --config /config.yaml
```

Leave the gateway running. A gateway routes messages; it does not answer A2A
requests itself.

### 2. Start The Echo Agent

In a second terminal, from the same workspace root:

```sh
cargo run -p a2acli-transport-slimrpc --release --locked --example echo-server
```

Wait for `Echo agent ready at org/demo/echo`. This
[example agent](examples/echo-server.rs) connects to `http://127.0.0.1:46357`,
uses the demo shared secret, and responds with `Echo: <your text>`. Its identity
is `org/demo/echo`; the plugin uses the distinct identity `org/demo/cli`.

### 3. Send A Message

Back in the build shell:

```sh
export A2A_SLIMRPC_PLUGIN_CONFIG="$PWD/a2acli-transport-slimrpc/examples/plugin.yaml"
a2a send --transport slimrpc --endpoint slim://org/demo/echo "hello over SLIMRPC"
```

The response should contain `Echo: hello over SLIMRPC`, and the command should
exit successfully. The [plugin configuration](examples/plugin.yaml) is the
complete YAML shown above. The CLI starts the plugin automatically; there is
no separate proxy process to start manually.

To check streaming:

```sh
a2a send --stream --transport slimrpc --endpoint slim://org/demo/echo "hello stream"
```

The response should contain `Echo: hello stream`, then the command should exit.
This example returns a single message and does not demonstrate a long-lived
task or task subscription.

### 4. Stop The Example

Press Ctrl-C in the echo-agent terminal and wait for the shell prompt. Then run
`docker stop a2a-slimrpc-demo`; `--rm` removes the stopped demo container.
Unset the shell configuration with `unset A2A_SLIMRPC_PLUGIN_CONFIG`.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| `slimrpc` is missing from `a2a transport list` | The executable must be named `a2a-transport-slimrpc`, be executable, and be on the CLI's `PATH`. Run `command -v a2a-transport-slimrpc`. |
| Error mentioning `A2A_SLIMRPC_PLUGIN_CONFIG` | Export an absolute path to a readable YAML file in the shell running `a2a`. |
| Invalid gateway scheme or TLS connection failure | Use `http://127.0.0.1:46357` and `tls.insecure: true` for the local demo. TLS settings must agree with the gateway. |
| Shared-secret setup failure | Use at least 32 bytes of secret data, with matching secrets in the plugin and agent. |
| Gateway connection failure or startup timeout | Start the gateway first; check `docker logs a2a-slimrpc-demo` and host port 46357. The Go CLI's plugin handshake has its own startup deadline. |
| Discovery works, but no agent response arrives | Ensure the echo agent is running and the target is `org/demo/echo`, not the plugin identity or gateway address. |

Set `RUST_LOG=debug` on the `a2a send` command to enable plugin diagnostics;
the plugin writes logs to stderr.

## Validation

```sh
cargo test -p a2acli-transport-slimrpc --release --locked
```

The [CLI plugin interop workflow](../.github/workflows/cli-plugin-interop.yml)
checks discovery with the real Go CLI and verifies the missing-configuration
startup error. Run the local example above to check unary and streaming
responses manually; the workflow does not start a gateway or echo agent.

## Workspace

This crate is part of the [A2A Rust SDK workspace](../README.md).
