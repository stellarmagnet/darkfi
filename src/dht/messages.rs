use rand::Rng;

use crate::{
    net,
    util::serial::{serialize, SerialDecodable, SerialEncodable},
};

/// This struct represents a DHT key request
#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct KeyRequest {
    /// Request id    
    pub id: blake3::Hash,
    /// Daemon id requesting the key
    pub from: blake3::Hash,
    /// Daemon id holding the key
    pub to: blake3::Hash,
    /// Key entry
    pub key: blake3::Hash,
}

impl KeyRequest {
    pub fn new(from: blake3::Hash, to: blake3::Hash, key: blake3::Hash) -> Self {
        // Generate a random id
        let mut rng = rand::thread_rng();
        let n: u16 = rng.gen();
        let id = blake3::hash(&serialize(&n));
        Self { id, from, to, key }
    }
}

impl net::Message for KeyRequest {
    fn name() -> &'static str {
        "keyrequest"
    }
}

/// This struct represents a DHT key request response
#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct KeyResponse {
    /// Response id
    pub id: blake3::Hash,
    /// Daemon id holding the key
    pub from: blake3::Hash,
    /// Daemon id holding the key
    pub to: blake3::Hash,
    /// Key entry
    pub key: blake3::Hash,
    /// Key value
    pub value: Vec<u8>,
}

impl KeyResponse {
    pub fn new(from: blake3::Hash, to: blake3::Hash, key: blake3::Hash, value: Vec<u8>) -> Self {
        // Generate a random id
        let mut rng = rand::thread_rng();
        let n: u16 = rng.gen();
        let id = blake3::hash(&serialize(&n));
        Self { id, from, to, key, value }
    }
}

impl net::Message for KeyResponse {
    fn name() -> &'static str {
        "keyresponse"
    }
}

/// This struct represents a lookup map request
#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct LookupRequest {
    /// Request id    
    pub id: blake3::Hash,
    /// Daemon id executing the request
    pub daemon: blake3::Hash,
    /// Key entry
    pub key: blake3::Hash,
    /// Request type
    pub req_type: u8, // 0 for insert, 1 for remove
}

impl LookupRequest {
    pub fn new(daemon: blake3::Hash, key: blake3::Hash, req_type: u8) -> Self {
        // Generate a random id
        let mut rng = rand::thread_rng();
        let n: u16 = rng.gen();
        let id = blake3::hash(&serialize(&n));
        Self { id, daemon, key, req_type }
    }
}

impl net::Message for LookupRequest {
    fn name() -> &'static str {
        "lookuprequest"
    }
}