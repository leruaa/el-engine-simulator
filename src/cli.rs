use clap::{command, Parser};

#[derive(Parser, Debug, Clone)]
#[command(author, version)]
pub struct Cli {
    #[arg(long, env)]
    pub el_endpoint_url: String,
}
