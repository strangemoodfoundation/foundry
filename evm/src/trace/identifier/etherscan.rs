use crate::trace::TraceIdentifier;
use ethers::{
    abi::Abi,
    etherscan::{errors::EtherscanError, Client},
    types::{Address, Chain},
};
use std::{borrow::Cow, collections::HashMap};

type Result<T> = std::result::Result<T, EtherscanError>;

#[derive(Clone, Debug)]
pub struct EtherscanIdentifier {
    /// The Etherscan client
    client: Client,
    ///
    identified: HashMap<Address, (String, Abi)>,
}

impl EtherscanIdentifier {
    pub fn new(chain: Chain, etherscan_api_key: String) -> Result<Self> {
        Ok(EtherscanIdentifier {
            client: Client::new(chain, etherscan_api_key)?,
            identified: Default::default(),
        })
    }

    pub async fn identify(&self, addr: Address) -> Result<Option<(String, Abi)>> {
        let mut metadata = self.client.contract_source_code(addr).await?;
        Ok(if let Some(item) = metadata.items.pop() {
            Some((item.contract_name, serde_json::from_str(&item.abi)?))
        } else {
            None
        })
    }

    pub fn set_identified(
        &mut self,
        identified: Vec<Option<(ethers::types::Address, String, ethers::abi::Abi)>>,
    ) {
        for item in identified {
            if let Some((address, label, abi)) = item {
                self.identified.insert(address, (label, abi));
            }
        }
    }
}

impl TraceIdentifier for EtherscanIdentifier {
    fn identify_address(
        &self,
        addr: &Address,
        _: Option<&Vec<u8>>,
    ) -> (Option<String>, Option<String>, Option<Cow<Abi>>) {
        if let Some((label, abi)) = self.identified.get(addr) {
            (None, Some(label.to_owned()), Some(Cow::Borrowed(abi)))
        } else {
            (None, None, None)
        }
    }
}
