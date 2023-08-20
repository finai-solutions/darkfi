/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::str::FromStr;

use darkfi_sdk::crypto::ContractId;
use darkfi_serial::{deserialize, serialize};
use log::{debug, error};
use tinyjson::JsonValue;

use darkfi::{
    rpc::jsonrpc::{
        ErrorCode::{InternalError, InvalidParams, ParseError},
        JsonError, JsonResponse, JsonResult,
    },
    runtime::vm_runtime::SMART_CONTRACT_ZKAS_DB_NAME,
    util::encoding::base64,
};

use crate::{server_error, Darkfid, RpcError};

impl Darkfid {
    // RPCAPI:
    // Queries the blockchain database for a block in the given slot.
    // Returns a readable block upon success.
    //
    // **Params:**
    // * `array[0]`: `u64` slot ID (as string)
    //
    // **Returns:**
    // * [`BlockInfo`](https://darkrenaissance.github.io/darkfi/development/darkfi/consensus/block/struct.BlockInfo.html)
    //   struct serialized into base64.
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.get_slot", "params": ["0"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn blockchain_get_slot(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let slot = match u64::from_str_radix(params[0].get::<String>().unwrap(), 10) {
            Ok(v) => v,
            Err(_) => return JsonError::new(ParseError, None, id).into(),
        };

        let blocks = match self.validator.read().await.blockchain.get_blocks_by_slot(&[slot]) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "darkfid::rpc::blockchain_get_slot", "Failed fetching block by slot: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        if blocks.is_empty() {
            return server_error(RpcError::UnknownSlot, id, None)
        }

        let block = base64::encode(&serialize(&blocks[0]));
        JsonResponse::new(JsonValue::String(block), id).into()
    }

    // RPCAPI:
    // Queries the blockchain database for a given transaction.
    // Returns a serialized `Transaction` object.
    //
    // **Params:**
    // * `array[0]`: Hex-encoded transaction hash string
    //
    // **Returns:**
    // * Serialized [`Transaction`](https://darkrenaissance.github.io/darkfi/development/darkfi/tx/struct.Transaction.html)
    //   object encoded with base64
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.get_tx", "params": ["TxHash"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "ABCD...", "id": 1}
    pub async fn blockchain_get_tx(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let tx_hash = params[0].get::<String>().unwrap();
        let tx_hash = match blake3::Hash::from_hex(tx_hash) {
            Ok(v) => v,
            Err(_) => return JsonError::new(ParseError, None, id).into(),
        };

        let txs = match self.validator.read().await.blockchain.transactions.get(&[tx_hash], true) {
            Ok(txs) => txs,
            Err(e) => {
                error!(target: "darkfid::rpc::blockchain_get_tx", "Failed fetching tx by hash: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };
        // This would be an logic error somewhere
        assert_eq!(txs.len(), 1);
        // and strict was used during .get()
        let tx = txs[0].as_ref().unwrap();

        let tx_enc = base64::encode(&serialize(tx));
        JsonResponse::new(JsonValue::String(tx_enc), id).into()
    }

    // RPCAPI:
    // Queries the blockchain database to find the last known slot
    //
    // **Params:**
    // * `None`
    //
    // **Returns:**
    // * `u64` ID of the last known slot, as string
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.last_known_slot", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "1234", "id": 1}
    pub async fn blockchain_last_known_slot(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let blockchain = { self.validator.read().await.blockchain.clone() };
        let Ok(last_slot) = blockchain.last() else {
            return JsonError::new(InternalError, None, id).into()
        };

        JsonResponse::new(JsonValue::String(last_slot.0.to_string()), id).into()
    }

    // RPCAPI:
    // Initializes a subscription to new incoming blocks.
    // Once a subscription is established, `darkfid` will send JSON-RPC notifications of
    // new incoming blocks to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.subscribe_blocks", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "blockchain.subscribe_blocks", "params": [`blockinfo`]}
    pub async fn blockchain_subscribe_blocks(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        self.subscribers.get("blocks").unwrap().clone().into()
    }

    // RPCAPI:
    // Initializes a subscription to new incoming transactions.
    // Once a subscription is established, `darkfid` will send JSON-RPC notifications of
    // new incoming transactions to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.subscribe_txs", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "blockchain.subscribe_txs", "params": [`tx_hash`]}
    pub async fn blockchain_subscribe_txs(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        self.subscribers.get("txs").unwrap().clone().into()
    }

    // RPCAPI:
    // Initializes a subscription to new incoming proposals, asuming node participates
    // in consensus. Once a subscription is established, `darkfid` will send JSON-RPC
    // notifications of new incoming proposals to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.subscribe_proposals", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "blockchain.subscribe_proposals", "params": [`blockinfo`]}
    pub async fn blockchain_subscribe_proposals(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Since proposals subscriber is only active if we participate to consensus,
        // we have to check if it actually exists in the subscribers map.
        let proposals_subscriber = self.subscribers.get("proposals");
        if proposals_subscriber.is_none() {
            error!(target: "darkfid::rpc::blockchain_subscribe_proposals", "Proposals subscriber not found");
            return JsonError::new(InternalError, None, id).into()
        }

        proposals_subscriber.unwrap().clone().into()
    }

    // RPCAPI:
    // Performs a lookup of zkas bincodes for a given contract ID and returns all of
    // them, including their namespace.
    //
    // **Params:**
    // * `array[0]`: base58-encoded contract ID string
    //
    // **Returns:**
    // * `array[n]`: Pairs of: `zkas_namespace` string, serialized
    //   [`ZkBinary`](https://darkrenaissance.github.io/darkfi/development/darkfi/zkas/decoder/struct.ZkBinary.html)
    //   object
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.lookup_zkas", "params": ["6Ef42L1KLZXBoxBuCDto7coi9DA2D2SRtegNqNU4sd74"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [["Foo", "ABCD..."], ["Bar", "EFGH..."]], "id": 1}
    pub async fn blockchain_lookup_zkas(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let contract_id = params[0].get::<String>().unwrap();
        let contract_id = match ContractId::from_str(contract_id) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "darkfid::rpc::blockchain_lookup_zkas", "Error decoding string to ContractId: {}", e);
                return JsonError::new(InvalidParams, None, id).into()
            }
        };

        let blockchain = { self.validator.read().await.blockchain.clone() };

        let Ok(zkas_db) = blockchain.contracts.lookup(
            &blockchain.sled_db,
            &contract_id,
            SMART_CONTRACT_ZKAS_DB_NAME,
        ) else {
            error!(
                target: "darkfid::rpc::blockchain_lookup_zkas", "Did not find zkas db for ContractId: {}",
                contract_id
            );
            return server_error(RpcError::ContractZkasDbNotFound, id, None)
        };

        let mut ret = vec![];

        for i in zkas_db.iter() {
            debug!(target: "darkfid::rpc::blockchain_lookup_zkas", "Iterating over zkas db");
            let Ok((zkas_ns, zkas_bytes)) = i else {
                error!(target: "darkfid::rpc::blockchain_lookup_zkas", "Internal sled error iterating db");
                return JsonError::new(InternalError, None, id).into()
            };

            let Ok(zkas_ns) = deserialize(&zkas_ns) else {
                return JsonError::new(InternalError, None, id).into()
            };

            let zkas_bincode = base64::encode(&zkas_bytes);
            ret.push(JsonValue::Array(vec![
                JsonValue::String(zkas_ns),
                JsonValue::String(zkas_bincode),
            ]));
        }

        JsonResponse::new(JsonValue::Array(ret), id).into()
    }
}
