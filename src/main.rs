mod cli;
mod engine_payload;

use anvil_rpc::request::RpcMethodCall;
use anvil_rpc::response::{ResponseResult, RpcResponse};
use anyhow::{anyhow, Error, Result};
use axum::body::Body;
use axum::extract::FromRequest;
use axum::http::Request;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json, Router, Server};
use clap::Parser;
use hyper::client::HttpConnector;
use hyper::{Client, Method, StatusCode};
use hyper_tls::HttpsConnector;
use reth_primitives::proofs::calculate_transaction_root;
use reth_primitives::{Header, TransactionSigned, EMPTY_OMMER_ROOT, H256, U256, U64};
use reth_rlp::Decodable;
use reth_rpc_types::engine::{
    ExecutionPayload, ForkchoiceState, ForkchoiceUpdated, PayloadAttributes, PayloadStatus,
    PayloadStatusEnum, TransitionConfiguration,
};

use serde_json::Value;
use tower_http::trace::TraceLayer;
use tracing::{error, info};

use crate::cli::Cli;
use crate::engine_payload::EnginePayload;

async fn index(
    Extension(args): Extension<Cli>,
    Extension(client): Extension<Client<HttpsConnector<HttpConnector>>>,
    request: Request<Body>,
) -> Result<Response, StatusCode> {
    let method = request.method().to_owned();
    let uri = request.uri().to_owned();

    match Json::<RpcMethodCall>::from_request(request, &()).await {
        Ok(json_rpc) => {
            let json_method = json_rpc.0.method.as_str();

            let response = match json_method {
                eth if eth.starts_with("eth") => {
                    proxy(
                        &args.el_endpoint_url,
                        &json_rpc,
                        &method,
                        uri.path(),
                        client,
                    )
                    .await
                }
                engine if engine.starts_with("engine") => simulator(&json_rpc).await,
                unknown => Err(anyhow!("Unknown method: {}", unknown)),
            };

            match response {
                Ok(response) => Ok(response),
                Err(err) => {
                    error!("'{:?}' for method {}", err, json_method);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
        Err(err) => {
            error!("'{:?}' on path {}", err, uri.path());
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn proxy(
    el_endpoint_url: &str,
    json_rpc: &RpcMethodCall,
    method: &Method,
    path: &str,
    client: Client<HttpsConnector<HttpConnector>>,
) -> Result<Response> {
    let request_url_and_path = if path == "/" {
        el_endpoint_url.to_owned()
    } else {
        el_endpoint_url.to_owned() + path
    };
    let body = serde_json::to_vec(json_rpc)?;

    let req = Request::builder()
        .uri(request_url_and_path)
        .method(method)
        .body(Body::from(body))?;

    client
        .request(req)
        .await
        .map(|r| r.into_response())
        .map_err(Error::from)
}

async fn simulator(json_rpc: &RpcMethodCall) -> Result<Response> {
    let payload = EnginePayload::try_from(json_rpc)?;

    let value = match payload {
        EnginePayload::ExchangeTransitionConfiguration(transition_configuration) => {
            exchange_transition_configuration(transition_configuration)
        }
        EnginePayload::NewPayload(execution_payload) => new_payload(execution_payload),
        EnginePayload::ForkChoiceUpdated {
            forkchoice_state,
            payload_attributes,
        } => forkchoice_updated(forkchoice_state, payload_attributes),
    }?;

    let response = RpcResponse::new(json_rpc.id(), ResponseResult::Success(value));

    Ok(Json(response).into_response())
}

fn exchange_transition_configuration(
    _transition_configuration: TransitionConfiguration,
) -> Result<Value> {
    let transition_configuration = TransitionConfiguration {
        terminal_total_difficulty: U256::from(58750000000000000000000_i128),
        terminal_block_hash: H256::zero(),
        terminal_block_number: U64::zero(),
    };

    serde_json::to_value(transition_configuration).map_err(Error::from)
}

fn new_payload(execution_payload: ExecutionPayload) -> Result<Value> {
    let transactions = execution_payload
        .transactions
        .clone()
        .iter_mut()
        .map(|tx| TransactionSigned::decode(&mut tx.as_ref()).unwrap())
        .collect::<Vec<_>>();

    let header = Header {
        parent_hash: execution_payload.parent_hash,
        ommers_hash: EMPTY_OMMER_ROOT,
        beneficiary: execution_payload.fee_recipient,
        state_root: execution_payload.state_root,
        transactions_root: calculate_transaction_root(transactions.iter()),
        receipts_root: execution_payload.receipts_root,
        logs_bloom: execution_payload.logs_bloom,
        difficulty: U256::from(0),
        number: execution_payload.block_number.as_u64(),
        gas_limit: execution_payload.gas_limit.as_u64(),
        gas_used: execution_payload.gas_used.as_u64(),
        timestamp: execution_payload.timestamp.as_u64(),
        mix_hash: execution_payload.prev_randao,
        nonce: 0,
        base_fee_per_gas: Some(execution_payload.base_fee_per_gas.to()),
        extra_data: execution_payload.extra_data,
    };

    let payload_status = if execution_payload.block_hash != header.hash_slow() {
        PayloadStatus {
            status: PayloadStatusEnum::InvalidBlockHash {
                validation_error: String::from(""),
            },
            latest_valid_hash: None,
        }
    } else {
        PayloadStatus {
            status: PayloadStatusEnum::Valid,
            latest_valid_hash: Some(execution_payload.block_hash),
        }
    };

    serde_json::to_value(payload_status).map_err(Error::from)
}

fn forkchoice_updated(
    forkchoice_state: ForkchoiceState,
    _payload_attributes: Option<PayloadAttributes>,
) -> Result<Value> {
    let forkchoice = ForkchoiceUpdated {
        payload_status: PayloadStatus {
            status: PayloadStatusEnum::Valid,
            latest_valid_hash: Some(forkchoice_state.head_block_hash),
        },
        payload_id: None,
    };

    serde_json::to_value(forkchoice).map_err(Error::from)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args = Cli::parse();
    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, Body>(https);

    info!("Starting server...");

    // Axum router
    let router = Router::new()
        .fallback(index)
        .layer(Extension(args))
        .layer(Extension(client))
        .layer(TraceLayer::new_for_http());

    // Start server
    Server::bind(&"0.0.0.0:80".parse().unwrap())
        .serve(router.into_make_service())
        .await
        .unwrap();
}
