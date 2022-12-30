use anvil_rpc::request::{RequestParams, RpcMethodCall};
use anyhow::{bail, Error, Result};
use reth_rpc_types::engine::{
    ExecutionPayload, ForkchoiceState, PayloadAttributes, TransitionConfiguration,
};

pub enum EnginePayload {
    ExchangeTransitionConfiguration(TransitionConfiguration),
    NewPayload(ExecutionPayload),
    ForkChoiceUpdated {
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    },
}

impl TryFrom<&RpcMethodCall> for EnginePayload {
    type Error = Error;

    fn try_from(value: &RpcMethodCall) -> Result<Self> {
        match value.method.as_str() {
            "engine_exchangeTransitionConfigurationV1" => {
                let transition_configuration = if let RequestParams::Array(params) = &value.params {
                    serde_json::from_value(params[0].clone())
                } else {
                    bail!("Invalid params")
                };
                Ok(EnginePayload::ExchangeTransitionConfiguration(
                    transition_configuration?,
                ))
            }
            "engine_newPayloadV1" => {
                let execution_payload = if let RequestParams::Array(params) = &value.params {
                    serde_json::from_value(params[0].clone())
                } else {
                    bail!("Invalid params")
                };
                Ok(EnginePayload::NewPayload(execution_payload?))
            }
            "engine_forkchoiceUpdatedV1" => {
                let (forkchoice_state, payload_attributes) =
                    if let RequestParams::Array(params) = &value.params {
                        (
                            serde_json::from_value(params[0].clone()),
                            serde_json::from_value(params[1].clone()),
                        )
                    } else {
                        bail!("Invalid params")
                    };
                Ok(EnginePayload::ForkChoiceUpdated {
                    forkchoice_state: forkchoice_state?,
                    payload_attributes: payload_attributes?,
                })
            }
            default => bail!("Unknown method: {}", default),
        }
    }
}
