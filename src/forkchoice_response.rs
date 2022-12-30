use reth_rpc_types::engine::PayloadStatus;
use serde::{Serialize, Deserialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkchoiceResponse {
    pub payload_status: PayloadStatus,
    pub payload_id: Option<Value>,
}