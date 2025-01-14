use async_std::sync::{Arc, Mutex};
use std::{
    env,
    fs::{create_dir_all, remove_dir_all},
    io::stdin,
};

use async_executor::Executor;
use crypto_box::{
    aead::{Aead, AeadCore},
    SalsaBox, SecretKey,
};
use futures::{select, FutureExt};
use fxhash::FxHashMap;
use log::{debug, error, info, warn};
use smol::future;
use structopt_toml::StructOptToml;

use darkfi::{
    async_daemonize, net,
    raft::{NetMsg, ProtocolRaft, Raft, RaftSettings},
    rpc::server::listen_and_serve,
    util::{
        cli::{get_log_config, get_log_level, spawn_config},
        expand_path,
        path::get_config_path,
        serial::{deserialize, serialize, SerialDecodable, SerialEncodable},
    },
    Error, Result,
};

mod error;
mod jsonrpc;
mod month_tasks;
mod settings;
mod task_info;
mod util;

use crate::{
    error::TaudResult,
    jsonrpc::JsonRpcInterface,
    settings::{Args, CONFIG_FILE, CONFIG_FILE_CONTENTS},
    task_info::TaskInfo,
    util::{parse_workspaces, Workspace},
};

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct EncryptedTask {
    workspace: String,
    nonce: Vec<u8>,
    payload: Vec<u8>,
}

fn encrypt_task(
    task: &TaskInfo,
    workspace: &String,
    salsa_box: &SalsaBox,
    rng: &mut crypto_box::rand_core::OsRng,
) -> TaudResult<EncryptedTask> {
    debug!("start encrypting task");

    let nonce = SalsaBox::generate_nonce(rng);
    let payload = &serialize(task)[..];
    let payload = salsa_box.encrypt(&nonce, payload)?;

    let nonce = nonce.to_vec();
    Ok(EncryptedTask { workspace: workspace.to_string(), nonce, payload })
}

fn decrypt_task(encrypt_task: &EncryptedTask, salsa_box: &SalsaBox) -> TaudResult<TaskInfo> {
    debug!("start decrypting task");

    let nonce = encrypt_task.nonce.as_slice();
    let decrypted_task = salsa_box.decrypt(nonce.into(), &encrypt_task.payload[..])?;

    let task = deserialize(&decrypted_task)?;

    Ok(task)
}

async fn start_sync_loop(
    commits_received: Arc<Mutex<Vec<String>>>,
    broadcast_rcv: async_channel::Receiver<TaskInfo>,
    raft_msgs_sender: async_channel::Sender<EncryptedTask>,
    commits_recv: async_channel::Receiver<EncryptedTask>,
    datastore_path: std::path::PathBuf,
    configured_ws: FxHashMap<String, Workspace>,
    mut rng: crypto_box::rand_core::OsRng,
) -> TaudResult<()> {
    loop {
        select! {
            task = broadcast_rcv.recv().fuse() => {
                let tk = task.map_err(Error::from)?;
                if configured_ws.contains_key(&tk.workspace) {
                    let ws_info = configured_ws.get(&tk.workspace).unwrap();
                    if let Some(salsa_box) = &ws_info.encryption {
                        let encrypted_task = encrypt_task(&tk, &tk.workspace, salsa_box, &mut rng)?;
                        info!(target: "tau", "Send the task: ref: {}", tk.ref_id);
                        raft_msgs_sender.send(encrypted_task).await.map_err(Error::from)?;
                    }
                }
            }
            task = commits_recv.recv().fuse() => {
                let recv = task.map_err(Error::from)?;
                if configured_ws.contains_key(&recv.workspace) {
                    let ws_info = configured_ws.get(&recv.workspace).unwrap();
                    if let Some(salsa_box) = &ws_info.encryption {
                        let task = decrypt_task(&recv, salsa_box);
                        if let Err(e) = task {
                            info!("unable to decrypt the task: {}", e);
                            continue
                        }

                        let task = task.unwrap();
                        if !commits_received.lock().await.contains(&task.ref_id) {
                            commits_received.lock().await.push(task.ref_id.clone());
                        }
                        info!(target: "tau", "Save the task: ref: {}", task.ref_id);
                        task.save(&datastore_path)?;
                    }
                }
            }
        }
    }
}

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<Executor<'_>>) -> Result<()> {
    let datastore_path = expand_path(&settings.datastore)?;

    let nickname =
        if settings.nickname.is_some() { settings.nickname } else { env::var("USER").ok() };

    if nickname.is_none() {
        error!("Provide a nickname in config file");
        return Ok(())
    }

    let mut rng = crypto_box::rand_core::OsRng;

    if settings.key_gen {
        info!(target: "tau", "Generating a new secret key");
        let secret_key = SecretKey::generate(&mut rng);
        let encoded = bs58::encode(secret_key.as_bytes());
        println!("Secret key: {}", encoded.into_string());
        return Ok(())
    }

    if settings.refresh {
        println!("Removing local data in: {:?} (yes/no)? ", datastore_path);
        let mut confirm = String::new();
        stdin().read_line(&mut confirm).ok().expect("Failed to read line");

        let confirm = confirm.to_lowercase();
        let confirm = confirm.trim();

        if confirm == "yes" || confirm == "y" {
            remove_dir_all(datastore_path).unwrap_or(());
            println!("Local data get removed");
        } else {
            error!("Unexpected Value: {}", confirm);
        }

        return Ok(())
    }

    // mkdir datastore_path if not exists
    create_dir_all(datastore_path.clone())?;
    create_dir_all(datastore_path.join("month"))?;
    create_dir_all(datastore_path.join("task"))?;

    // Pick up workspace settings from the TOML configuration
    let cfg_path = get_config_path(settings.config, CONFIG_FILE)?;
    let configured_ws = parse_workspaces(&cfg_path)?;

    // start at the first configured workspace
    let workspace = if let Some(key) = configured_ws.keys().next() {
        Arc::new(Mutex::new(key.to_owned()))
    } else {
        error!("Please provide at least one workspace in the config file: {:?}", cfg_path);
        return Ok(())
    };

    //
    // Raft
    //
    let net_settings = settings.net;
    let seen_net_msgs = Arc::new(Mutex::new(FxHashMap::default()));

    let datastore_raft = datastore_path.join("tau.db");
    let raft_settings = RaftSettings { datastore_path: datastore_raft, ..RaftSettings::default() };

    let mut raft = Raft::<EncryptedTask>::new(raft_settings, seen_net_msgs.clone())?;
    let raft_id = raft.id();

    let commits_received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));

    let (broadcast_snd, broadcast_rcv) = async_channel::unbounded::<TaskInfo>();

    //
    // P2p setup
    //
    let (p2p_send_channel, p2p_recv_channel) = async_channel::unbounded::<NetMsg>();

    let p2p = net::P2p::new(net_settings.into()).await;
    let p2p = p2p.clone();
    let registry = p2p.protocol_registry();

    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let raft_id = raft_id.clone();
            let sender = p2p_send_channel.clone();
            let seen_net_msgs_cloned = seen_net_msgs.clone();
            async move {
                ProtocolRaft::init(raft_id, channel, sender, p2p, seen_net_msgs_cloned).await
            }
        })
    .await;

    p2p.clone().start(executor.clone()).await?;

    executor.spawn(p2p.clone().run(executor.clone())).detach();

    p2p.clone().wait_for_outbound().await?;

    //
    // RPC interface
    //
    let rpc_interface = Arc::new(JsonRpcInterface::new(
        datastore_path.clone(),
        broadcast_snd,
        nickname.unwrap(),
        workspace,
        configured_ws.clone(),
        p2p.clone(),
    ));
    executor.spawn(listen_and_serve(settings.rpc_listen.clone(), rpc_interface)).detach();

    //
    // Waiting Exit signal
    //
    let (signal, shutdown) = async_channel::bounded::<()>(1);
    ctrlc::set_handler(move || {
        warn!(target: "tau", "Catch exit signal");
        // cleaning up tasks running in the background
        if let Err(e) = async_std::task::block_on(signal.send(())) {
            error!("Error on sending exit signal: {}", e);
        }
    })
    .unwrap();

    executor
        .spawn(start_sync_loop(
            commits_received.clone(),
            broadcast_rcv,
            raft.sender(),
            raft.receiver(),
            datastore_path,
            configured_ws,
            rng,
        ))
        .detach();

    raft.run(p2p.clone(), p2p_recv_channel.clone(), executor.clone(), shutdown.clone()).await?;

    Ok(())
}
