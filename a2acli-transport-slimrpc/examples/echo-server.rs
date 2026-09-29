// Copyright AGNTCY Contributors (https://github.com/agntcy)
// Copyright A2A Contributors (https://github.com/a2aproject)
// SPDX-License-Identifier: Apache-2.0

//! Local SLIMRPC echo agent for the plugin README's end-to-end example.
//!
//! Start the SLIM gateway first, then run from the workspace root:
//! cargo run -p a2acli-transport-slimrpc --example echo-server --locked

use std::sync::Arc;

use a2a::{A2AError, AgentCapabilities, Message, Part, Role, StreamResponse};
use a2a_server::{AgentExecutor, DefaultRequestHandler, ExecutorContext, InMemoryTaskStore};
use a2a_slimrpc::SlimRpcHandler;
use futures::stream::{self, BoxStream};
use slim_auth::auth_provider::{AuthProvider, AuthVerifier};
use slim_auth::shared_secret::SharedSecret;
use slim_config::client::ClientConfig;
use slim_config::component::id::ID;
use slim_config::tls::client::TlsClientConfig;
use slim_datapath::api::ProtoName;
use slim_rpc::Server;
use slim_service::service::{Service, ServiceBuilder};

const GATEWAY: &str = "http://127.0.0.1:46357";
// Public demo credential: use only with the local gateway in the README.
const SHARED_SECRET: &str = "slimrpc-local-demo-secret-at-least-32-bytes";

struct EchoExecutor;

impl AgentExecutor for EchoExecutor {
    fn execute(
        &self,
        ctx: ExecutorContext,
    ) -> BoxStream<'static, Result<StreamResponse, A2AError>> {
        let text = ctx
            .message
            .as_ref()
            .and_then(Message::text)
            .unwrap_or_default();
        let reply = Message::new(Role::Agent, vec![Part::text(format!("Echo: {text}"))]);
        Box::pin(stream::iter([Ok(StreamResponse::Message(reply))]))
    }

    fn cancel(
        &self,
        _ctx: ExecutorContext,
    ) -> BoxStream<'static, Result<StreamResponse, A2AError>> {
        Box::pin(stream::iter([Err(A2AError::unsupported_operation(
            "the echo agent returns messages immediately",
        ))]))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .ok();

    let service = Service::new(ID::new_with_name(ServiceBuilder::kind(), "echo-server")?);
    let client = ClientConfig::with_endpoint(GATEWAY).with_tls_setting(TlsClientConfig::insecure());
    let connection = service.connect(&client).await?;

    let name = ProtoName::from_strings(["org", "demo", "echo"]);
    let secret = SharedSecret::new("demo-agent", SHARED_SECRET)?;
    let (app, notifications) = service.create_app(
        &name,
        AuthProvider::shared_secret(secret.clone()),
        AuthVerifier::shared_secret(secret),
    )?;
    let app = Arc::new(app);
    app.subscribe(&name, Some(connection)).await?;

    let server =
        Server::new_with_connection_and_runtime(app, name, Some(connection), notifications, None);
    let handler = DefaultRequestHandler::new(EchoExecutor, InMemoryTaskStore::new())
        .with_capabilities(AgentCapabilities {
            streaming: Some(true),
            ..Default::default()
        });
    SlimRpcHandler::new(Arc::new(handler)).register(&server);

    println!("Echo agent ready at org/demo/echo via {GATEWAY}; press Ctrl-C to stop.");
    let serve = server.serve();
    tokio::pin!(serve);
    tokio::select! {
        result = &mut serve => result?,
        signal = tokio::signal::ctrl_c() => {
            signal?;
            server.shutdown().await;
            serve.await?;
        }
    }
    service.shutdown().await?;
    Ok(())
}
