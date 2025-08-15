#![warn(clippy::panic)]

use std::str::FromStr;

use anyhow::anyhow;
use clap::Parser;
use readyset_client::ReadySetHandle;
use readyset_client::consensus::AuthorityType;

#[derive(Parser)]
#[command(name = "controller_request")]
struct ControllerRequest {
    #[arg(short, long, env("AUTHORITY_ADDRESS"), default_value("127.0.0.1:8500"))]
    authority_address: String,

    #[arg(long, env("AUTHORITY"), default_value("consul"), value_parser = ["consul"])]
    authority: AuthorityType,

    #[arg(short, long, env("DEPLOYMENT"))]
    deployment: String,

    /// The name of the endpoint to issue a controller request to.
    /// This currently only supports endpoints without parameters.
    #[arg(short, long)]
    endpoint: Request,
}

#[derive(Clone, Copy, Debug)]
enum Request {
    HealthyWorkers,
    ControllerUri,
}

impl FromStr for Request {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "/healthy_workers" => Ok(Request::HealthyWorkers),
            "/controller_uri" => Ok(Request::ControllerUri),
            _ => Err(anyhow!("Unsupported request")),
        }
    }
}

impl Request {
    async fn issue_and_print(&self, mut handle: ReadySetHandle) -> anyhow::Result<()> {
        match self {
            Request::HealthyWorkers => {
                let res = handle.healthy_workers().await?;
                println!("{res:?}");
            }
            Request::ControllerUri => {
                let res = handle.controller_uri().await?;
                println!("{res:?}");
            }
        }

        Ok(())
    }
}

impl ControllerRequest {
    pub async fn run_command(self) -> anyhow::Result<()> {
        let authority = self
            .authority
            .to_authority(&self.authority_address, &self.deployment);

        let mut handle: ReadySetHandle = ReadySetHandle::new(authority).await;
        handle.ready().await.unwrap();

        self.endpoint.issue_and_print(handle).await?;

        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let controller_requester = ControllerRequest::parse();
    controller_requester.run_command().await
}
