use async_std::sync::Arc;

use async_executor::Executor;
use async_trait::async_trait;
use log::{debug, error};
use url::Url;

use crate::{
    consensus::{ValidatorStatePtr, Vote},
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Result,
};

pub struct ProtocolVote {
    vote_sub: MessageSubscription<Vote>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
    sync_p2p: P2pPtr,
    consensus_p2p: P2pPtr,
    channel_address: Url,
}

impl ProtocolVote {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        sync_p2p: P2pPtr,
        consensus_p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        debug!("Adding ProtocolVote to the protocol registry");
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<Vote>().await;

        let vote_sub = channel.subscribe_msg::<Vote>().await?;
        let channel_address = channel.address();

        Ok(Arc::new(Self {
            vote_sub,
            jobsman: ProtocolJobsManager::new("VoteProtocol", channel),
            state,
            sync_p2p,
            consensus_p2p,
            channel_address,
        }))
    }

    async fn handle_receive_vote(self: Arc<Self>) -> Result<()> {
        debug!("ProtocolVote::handle_receive_vote() [START]");
        let exclude_list = vec![self.channel_address.clone()];
        loop {
            let vote = match self.vote_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!("ProtocolVote::handle_receive_vote(): recv fail: {}", e);
                    continue
                }
            };

            debug!("ProtocolVote::handle_receive_vote() recv: {:?}", vote);

            let vote_copy = (*vote).clone();

            let (voted, to_broadcast) =
                match self.state.write().await.receive_vote(&vote_copy).await {
                    Ok(v) => v,
                    Err(e) => {
                        error!("handle_receive_vote(): receive_vote() fail: {}", e);
                        continue
                    }
                };

            if voted {
                if let Err(e) =
                    self.consensus_p2p.broadcast_with_exclude(vote_copy, &exclude_list).await
                {
                    error!("handle_receive_vote(): consensus p2p broadcast fail: {}", e);
                    continue
                };

                // Broadcast finalized blocks info, if any
                if let Some(blocks) = to_broadcast {
                    debug!("handle_receive_vote(): Broadcasting finalized blocks");
                    for info in blocks {
                        if let Err(e) = self.sync_p2p.broadcast(info).await {
                            error!("handle_receive_vote(): sync p2p broadcast fail: {}", e);
                            // TODO: Should we quit broadcasting if one fails?
                            continue
                        }
                    }
                } else {
                    debug!("handle_receive_vote(): No finalized blocks to broadcast");
                };
            }
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolVote {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!("ProtocolVote::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_vote(), executor.clone()).await;
        debug!("ProtocolVote::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolVote"
    }
}
