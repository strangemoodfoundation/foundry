use super::TraceIdentifier;
use ethers::{
    abi::{Abi, Address},
    etherscan,
    types::Chain,
};
use foundry_utils::RuntimeOrHandle;
use futures::{
    channel::mpsc::{channel, Receiver, Sender},
    stream::{Fuse, Stream, StreamExt},
    task::{Context, Poll},
    Future, FutureExt,
};
use std::{
    borrow::Cow,
    collections::{btree_map::Entry, BTreeMap},
    pin::Pin,
    sync::mpsc::{channel as oneshot_channel, Sender as OneshotSender},
};

type EtherscanRequest = Pin<Box<dyn Future<Output = Option<(String, Abi)>> + Send>>;
type HandlerRequest = (Address, OneshotSender<Option<(String, Abi)>>);
type Listeners = BTreeMap<Address, Vec<OneshotSender<Option<(String, Abi)>>>>;
struct EtherscanHandler {
    /// The Etherscan client
    client: etherscan::Client,
    /// Cached information about addresses
    cache: BTreeMap<Address, Option<(String, Abi)>>,
    /// Incoming requests
    incoming: Fuse<Receiver<HandlerRequest>>,
    /// Requests currently in progress
    pending_requests: Vec<(Address, EtherscanRequest)>,
    /// Requests we haven't responded to yet
    waiting: Listeners,
}

impl EtherscanHandler {
    pub fn new(client: etherscan::Client, incoming: Receiver<HandlerRequest>) -> Self {
        Self {
            client,
            incoming: incoming.fuse(),
            cache: BTreeMap::new(),
            pending_requests: Vec::new(),
            waiting: BTreeMap::new(),
        }
    }
}

impl Future for EtherscanHandler {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let pin = self.get_mut();

        // Pull incoming requests
        while let Poll::Ready(Some((addr, sender))) = Pin::new(&mut pin.incoming).poll_next(cx) {
            match pin.cache.entry(addr) {
                // If we have this address in the cache we just respond
                Entry::Occupied(entry) => {
                    let _ = sender.send(entry.get().clone());
                }
                // Otherwise we send a request to Etherscan
                Entry::Vacant(_) => {
                    let client = pin.client.clone();
                    pin.pending_requests.push((
                        addr,
                        Box::pin(async move {
                            client.contract_source_code(addr).await.ok().and_then(|mut metadata| {
                                if let Some(item) = metadata.items.pop() {
                                    Some((
                                        item.contract_name,
                                        serde_json::from_str(&item.abi).ok()?,
                                    ))
                                } else {
                                    None
                                }
                            })
                        }),
                    ));
                    pin.waiting.entry(addr).or_default().push(sender);
                }
            }
        }

        // Poll pending requests
        for n in (0..pin.pending_requests.len()).rev() {
            let (address, mut fut) = pin.pending_requests.swap_remove(n);
            if let Poll::Ready(info) = fut.poll_unpin(cx) {
                // Update cache
                pin.cache.insert(address, info.clone());

                // Notify all listeners
                if let Some(listeners) = pin.waiting.remove(&address) {
                    listeners.into_iter().for_each(|l| {
                        let _ = l.send(info.clone());
                    })
                }
                continue
            }
            pin.pending_requests.push((address, fut));
        }

        // Check if we're done
        if pin.incoming.is_done() && pin.pending_requests.is_empty() {
            return Poll::Ready(())
        }

        Poll::Pending
    }
}

pub struct EtherscanIdentifier {
    backend: Option<Sender<HandlerRequest>>,
}

impl EtherscanIdentifier {
    // TODO: Docs
    pub fn new(chain: Chain, etherscan_api_key: String) -> Self {
        if let Ok(client) = etherscan::Client::new(chain, etherscan_api_key) {
            let (backend, backend_rx) = channel(1);
            let handler = EtherscanHandler::new(client, backend_rx);

            let rt = RuntimeOrHandle::new();
            std::thread::spawn(move || match rt {
                RuntimeOrHandle::Runtime(runtime) => runtime.block_on(handler),
                RuntimeOrHandle::Handle(handle) => handle.block_on(handler),
            });

            Self { backend: Some(backend) }
        } else {
            Self { backend: None }
        }
    }
}

impl TraceIdentifier for EtherscanIdentifier {
    // TODO: Identify many at the same time
    fn identify_address(
        &self,
        addr: &Address,
        _: Option<&Vec<u8>>,
    ) -> (Option<String>, Option<String>, Option<Cow<Abi>>) {
        if let Some(backend) = &self.backend {
            let (sender, rx) = oneshot_channel();
            if backend.clone().try_send((*addr, sender)).is_ok() {
                if let Ok(Some((label, abi))) = rx.recv() {
                    return (None, Some(label), Some(Cow::Owned(abi)))
                }
            }
        }
        (None, None, None)
    }
}
