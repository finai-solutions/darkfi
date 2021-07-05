use crate::crypto::{coin::Coin, merkle::IncrementalWitness, merkle_node::MerkleNode, note::Note};
use crate::serial;
use crate::serial::{deserialize, serialize, Decodable, Encodable};
use crate::util::join_config_path;
use crate::Result;

use async_std::sync::{Arc, Mutex};
use ff::Field;
use log::*;
use rand::rngs::OsRng;
use rusqlite::{named_params, params, Connection};

use std::path::PathBuf;

pub type WalletPtr = Arc<WalletDB>;

pub struct WalletDB {
    pub path: PathBuf,
    pub secrets: Vec<jubjub::Fr>,
    pub cashier_secrets: Vec<jubjub::Fr>,
    pub coins: Mutex<Vec<Coin>>,
    pub notes: Mutex<Vec<Note>>,
    pub witnesses: Mutex<Vec<IncrementalWitness<MerkleNode>>>,
    pub cashier_public: jubjub::SubgroupPoint,
    pub public: jubjub::SubgroupPoint,
}

impl WalletDB {
    pub fn new(wallet: &str) -> Result<Self> {
        debug!(target: "walletdb", "new() Constructor called");
        let path = join_config_path(&PathBuf::from(wallet))?;
        let cashier_secret = jubjub::Fr::random(&mut OsRng);
        let secret = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let cashier_public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * cashier_secret;
        let coins = Mutex::new(Vec::new());
        let notes = Mutex::new(Vec::new());
        let witnesses = Mutex::new(Vec::new());
        Ok(Self {
            path,
            cashier_secrets: vec![cashier_secret.clone()],
            secrets: vec![secret.clone()],
            cashier_public,
            public,
            coins,
            notes,
            witnesses,
            //conn,
        })
    }

    pub async fn init_db(&self) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        debug!(target: "walletdb", "OPENED CONNECTION AT PATH {:?}", self.path);
        let contents = include_str!("../../res/schema.sql");
        match conn.execute_batch(&contents) {
            Ok(v) => println!("Database initalized successfully {:?}", v),
            Err(err) => println!("Error: {}", err),
        };
        Ok(())
    }

    pub async fn init_cashier_db(&self) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        debug!(target: "walletdb", "OPENED CONNECTION AT PATH {:?}", self.path);
        let contents = include_str!("../../res/schema.sql");
        match conn.execute_batch(&contents) {
            Ok(v) => println!("Database initalized successfully {:?}", v),
            Err(err) => println!("Error: {}", err),
        };
        Ok(())
    }

    pub async fn put_own_coins(
        &self,
        coin: Coin,
        note: Note,
        witness: IncrementalWitness<MerkleNode>,
        secret: jubjub::Fr,
    ) -> Result<()> {
        // prepare the values
        let coin = self.get_value_serialized(&coin.repr).await?;
        let serial = self.get_value_serialized(&note.serial).await?;
        let coin_blind = self.get_value_serialized(&note.coin_blind).await?;
        let valcom_blind = self.get_value_serialized(&note.valcom_blind).await?;
        let value = self.get_value_serialized(&note.value).await?;
        let asset_id = self.get_value_serialized(&note.asset_id).await?;
        let witness = self.get_value_serialized(&witness).await?;
        let secret = self.get_value_serialized(&secret).await?;
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        let mut unlock = conn.prepare("PRAGMA key = 'testkey'")?;
        let _rows = unlock.query([])?;
        // return key_id from key_private
        let mut get_id =
            conn.prepare("SELECT key_id FROM keys WHERE key_private = :key_private")?;
        let rows = get_id.query_map::<u8, _, _>(&[(":key_private", &secret)], |row| row.get(0))?;
        let mut key_id = Vec::new();
        for id in rows {
            key_id.push(id?)
        }
        conn.execute(
            "INSERT INTO coins(coin, serial, value, asset_id, coin_blind, valcom_blind, witness, key_id)
            VALUES (:coin, :serial, :value, :asset_id, :coin_blind, :valcom_blind, :witness, :key_id)",
            named_params! {
            ":coin": coin,
            ":serial": serial,
            ":value": value,
            ":asset_id": asset_id,
            ":coin_blind": coin_blind,
            ":valcom_blind": valcom_blind,
            ":witness": witness,
            ":key_id": key_id.pop().expect("key_id not found!"),
            },
        )?;
        Ok(())
    }

    pub async fn key_gen(&self) -> (Vec<u8>, Vec<u8>) {
        debug!(target: "key_gen", "Attempting to generate keys...");
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let pubkey = serial::serialize(&public);
        let privkey = serial::serialize(&secret);
        (pubkey, privkey)
    }

    pub async fn cash_key_gen(&self) -> (Vec<u8>, Vec<u8>) {
        debug!(target: "cash key_gen", "Generating cashier keys...");
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let pubkey = serial::serialize(&public);
        let privkey = serial::serialize(&secret);
        (pubkey, privkey)
    }

    pub async fn put_keypair(&self, key_public: Vec<u8>, key_private: Vec<u8>) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        let mut stmt = conn.prepare("PRAGMA key = 'testkey'")?;
        let _rows = stmt.query([])?;
        conn.execute(
            "INSERT INTO keys(key_public, key_private) VALUES (?1, ?2)",
            params![key_public, key_private],
        )?;
        Ok(())
    }

    pub async fn put_cashier_pub(&self, key_public: Vec<u8>) -> Result<()> {
        debug!(target: "save_cash_key", "Save cashier keys...");
        let conn = Connection::open(&self.path)?;
        let mut stmt = conn.prepare("PRAGMA key = 'testkey'")?;
        let _rows = stmt.query([])?;
        conn.execute(
            "INSERT INTO cashier(key_public) VALUES (?1)",
            params![key_public],
        )?;
        Ok(())
    }

    pub async fn get_public(&self) -> Result<Vec<u8>> {
        debug!(target: "get", "Returning keys...");
        let conn = Connection::open(&self.path)?;
        let mut stmt = conn.prepare("PRAGMA key = 'testkey'")?;
        let _rows = stmt.query([])?;
        let mut stmt = conn.prepare("SELECT key_public FROM keys")?;
        let key_iter = stmt.query_map::<u8, _, _>([], |row| row.get(0))?;
        let mut pub_keys = Vec::new();
        for key in key_iter {
            pub_keys.push(key?);
        }
        Ok(pub_keys)
    }

    pub async fn get_cashier_public(&self) -> Result<Vec<u8>> {
        debug!(target: "get_cashier_public", "Returning keys...");
        let conn = Connection::open(&self.path)?;
        let mut stmt = conn.prepare("PRAGMA key = 'testkey'")?;
        let _rows = stmt.query([])?;
        let mut stmt = conn.prepare("SELECT key_public FROM cashier")?;
        let key_iter = stmt.query_map::<u8, _, _>([], |row| row.get(0))?;
        let mut pub_keys = Vec::new();
        for key in key_iter {
            pub_keys.push(key?);
        }
        Ok(pub_keys)
    }

    pub fn get_private(&self) -> Result<Vec<u8>> {
        debug!(target: "get", "Returning keys...");
        let conn = Connection::open(&self.path)?;
        let mut stmt = conn.prepare("PRAGMA key = 'testkey'")?;
        let _rows = stmt.query([])?;
        let mut stmt = conn.prepare("SELECT key_private FROM keys")?;
        let key_iter = stmt.query_map::<u8, _, _>([], |row| row.get(0))?;
        let mut keys = Vec::new();
        for key in key_iter {
            keys.push(key?);
        }
        Ok(keys)
    }

    pub fn test_wallet(&self) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        let mut stmt = conn.prepare("PRAGMA key = 'testkey'")?;
        let _rows = stmt.query([])?;
        let mut stmt = conn.prepare("SELECT * FROM keys")?;
        let _rows = stmt.query([])?;
        Ok(())
    }

    pub async fn get_value_serialized<T: Encodable>(&self, data: &T) -> Result<Vec<u8>> {
        let v = serialize(data);
        Ok(v)
    }
    pub async fn get_value_deserialized<D: Decodable>(&self, key: Vec<u8>) -> Result<D> {
        let v: D = deserialize(&key)?;
        Ok(v)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    pub fn test_keypair() -> Result<()> {
        let path = join_config_path(&PathBuf::from("wallet.db"))?;
        let conn = Connection::open(path)?;
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let key_public = serial::serialize(&public);
        let key_private = serial::serialize(&secret);
        let mut stmt = conn.prepare("PRAGMA key = 'testkey'")?;
        let _rows = stmt.query([])?;
        conn.execute(
            "INSERT INTO keys(key_public, key_private) VALUES (?1, ?2)",
            params![key_public, key_private],
        )?;
        Ok(())
    }

    #[test]
    pub fn test_get_id() -> Result<()> {
        let path = join_config_path(&PathBuf::from("wallet.db"))?;
        let conn = Connection::open(path)?;
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let key_private = serial::serialize(&secret);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let key_public = serial::serialize(&public);
        let mut stmt = conn.prepare("PRAGMA key = 'testkey'")?;
        let _rows = stmt.query([])?;
        conn.execute(
            "INSERT INTO keys(key_public, key_private) VALUES (?1, ?2)",
            params![key_public, key_private],
        )?;
        let mut get_id =
            conn.prepare("SELECT key_id FROM keys WHERE key_private = :key_private")?;
        let rows =
            get_id.query_map::<u8, _, _>(&[(":key_private", &key_private)], |row| row.get(0))?;
        let mut key_id = Vec::new();
        for id in rows {
            key_id.push(id?)
        }
        println!("FOUND ID: {:?}", key_id.pop().unwrap());
        Ok(())
    }

    #[test]
    pub fn test_own_coins() -> Result<()> {
        let key_private = Vec::new();
        let coin = Vec::new();
        let serial = Vec::new();
        let coin_blind = Vec::new();
        let valcom_blind = Vec::new();
        let value = Vec::new();
        let asset_id = Vec::new();
        let witness = Vec::new();
        let path = join_config_path(&PathBuf::from("wallet.db"))?;
        let conn = Connection::open(path)?;
        let contents = include_str!("../../res/schema.sql");
        match conn.execute_batch(&contents) {
            Ok(v) => println!("Database initalized successfully {:?}", v),
            Err(err) => println!("Error: {}", err),
        };
        let mut unlock = conn.prepare("PRAGMA key = 'testkey'")?;
        let _rows = unlock.query([])?;
        let mut get_id =
            conn.prepare("SELECT key_id FROM keys WHERE key_private = :key_private")?;
        let rows =
            get_id.query_map::<u8, _, _>(&[(":key_private", &key_private)], |row| row.get(0))?;
        let mut key_id = Vec::new();
        for id in rows {
            key_id.push(id?)
        }
        conn.execute(
            "INSERT INTO coins(coin, serial, value, asset_id, coin_blind, valcom_blind, witness, key_id)
            VALUES (:coin, :serial, :value, :asset_id, :coin_blind, :valcom_blind, :witness, :key_id)",
            named_params! {
            ":coin": coin,
            ":serial": serial,
            ":value": value,
            ":asset_id": asset_id,
            ":coin_blind": coin_blind,
            ":valcom_blind": valcom_blind,
            ":witness": witness,
            ":key_id": key_id.pop().expect("key_id not found!"),
            },
        )?;
        Ok(())
    }
}
