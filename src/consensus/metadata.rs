use super::{Participant, Vote};
use crate::util::serial::{SerialDecodable, SerialEncodable};

/// This struct represents [`Block`](super::Block) information used by the Ouroboros
/// Praos consensus protocol.
#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Metadata {
    /// Proof that the stakeholder is the block owner
    pub proof: String,
    /// Random seed for VRF
    pub rand_seed: String,
    /// Block owner signature
    pub signature: String,
    /// Nodes participating in the consensus process
    pub participants: Vec<Participant>,
}

impl Metadata {
    pub fn new(
        proof: String,
        rand_seed: String,
        signature: String,
        participants: Vec<Participant>,
    ) -> Self {
        Self { proof, rand_seed, signature, participants }
    }
}

/// This struct represents [`Block`](super::Block) information used by the Streamlet
/// consensus protocol.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct StreamletMetadata {
    /// Slot votes
    pub votes: Vec<Vote>,
    /// Block notarization flag
    pub notarized: bool,
    /// Block finalization flag
    pub finalized: bool,
}

impl StreamletMetadata {
    pub fn new() -> Self {
        Self { votes: vec![], notarized: false, finalized: false }
    }
}
