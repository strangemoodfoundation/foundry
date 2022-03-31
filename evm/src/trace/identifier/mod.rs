/// A trace identifier that tries to identify addresses using local contracts.
pub mod local;
pub use local::LocalTraceIdentifier;

/// A trace identifier that tries to identify addresses using Etherscan.
pub mod etherscan;
pub use etherscan::EtherscanIdentifier;

use ethers::abi::{Abi, Address};
use std::borrow::Cow;

/// Trace identifiers figure out what ABIs and labels belong to all the addresses of the trace.
pub trait TraceIdentifier {
    /// Attempts to identify an address in one or more call traces.
    ///
    /// The tuple is of the format `(contract, label, abi)`, where `contract` is intended to be of
    /// the format `"<artifact>:<contract>"`, e.g. `"Foo.json:Foo"`.
    fn identify_address(
        &self,
        address: &Address,
        code: Option<&Vec<u8>>,
    ) -> (Option<String>, Option<String>, Option<Cow<Abi>>);
}
