use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc::Receiver;

use super::{ClientHandle, ClientId, ToServer};

#[derive(Default, Debug, Clone)]
struct ClientState {}

#[derive(Default, Debug)]
struct WorldServer {
    to_remove: Vec<ClientId>,
    clients: HashMap<ClientId, (ClientHandle, ClientState)>,
}

pub async fn server_main_loop(mut recv: Receiver<ToServer>) -> Result<(), std::io::Error> {
    let data = Arc::new(Mutex::new(WorldServer::default()));

    while let Some(msg) = recv.recv().await {
        let mut to_remove = Vec::new();

        match msg {
            ToServer::Message(_, _) => todo!(),
            ToServer::NewClient(handle) => {
                let mut data = data.lock().unwrap();

                data.clients
                    .insert(handle.id, (handle, ClientState::default()));
            }
            ToServer::Disconnected(from_id) => {
                let mut data = data.lock().unwrap();

                data.to_remove.push(from_id);
            }
            ToServer::FatalError(err) => return Err(err),
        }

        // Remove any clients that errored out
        {
            let mut data = data.lock().unwrap();
            data.to_remove.append(&mut to_remove);

            for remove_id in data.to_remove.clone() {
                data.clients.remove(&remove_id);
            }
        }
    }
    Ok(())
}
