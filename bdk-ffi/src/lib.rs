mod bitcoin;
mod descriptor;
mod esplora;
mod keys;
mod wallet;

// TODO 6: Why are these imports required?
use crate::bitcoin::Address;
use crate::bitcoin::Network;
use crate::bitcoin::OutPoint;
use crate::bitcoin::PartiallySignedTransaction;
use crate::bitcoin::Script;
use crate::bitcoin::Transaction;
use crate::descriptor::Descriptor;
use crate::esplora::EsploraClient;
use crate::keys::DerivationPath;
use crate::keys::DescriptorPublicKey;
use crate::keys::DescriptorSecretKey;
use crate::keys::Mnemonic;
use crate::wallet::TxBuilder;
use crate::wallet::Update;
use crate::wallet::Wallet;

use bdk::keys::bip39::WordCount;
use bdk::wallet::tx_builder::ChangeSpendPolicy;
use bdk::wallet::AddressIndex as BdkAddressIndex;
use bdk::wallet::AddressInfo as BdkAddressInfo;
use bdk::wallet::Balance as BdkBalance;
use bdk::Error as BdkError;
use bdk::KeychainKind;

use std::sync::Arc;

uniffi::include_scaffolding!("bdk");

/// A output script and an amount of satoshis.
pub struct ScriptAmount {
    pub script: Arc<Script>,
    pub amount: u64,
}

/// A derived address and the index it was found at.
pub struct AddressInfo {
    /// Child index of this address.
    pub index: u32,
    /// Address.
    pub address: Arc<Address>,
    /// Type of keychain.
    pub keychain: KeychainKind,
}

impl From<BdkAddressInfo> for AddressInfo {
    fn from(address_info: BdkAddressInfo) -> Self {
        AddressInfo {
            index: address_info.index,
            address: Arc::new(address_info.address.into()),
            keychain: address_info.keychain,
        }
    }
}

/// The address index selection strategy to use to derived an address from the wallet's external
/// descriptor.
pub enum AddressIndex {
    /// Return a new address after incrementing the current descriptor index.
    New,
    /// Return the address for the current descriptor index if it has not been used in a received
    /// transaction. Otherwise return a new address as with AddressIndex::New.
    /// Use with caution, if the wallet has not yet detected an address has been used it could
    /// return an already used address. This function is primarily meant for situations where the
    /// caller is untrusted; for example when deriving donation addresses on-demand for a public
    /// web page.
    LastUnused,
    /// Return the address for a specific descriptor index. Does not change the current descriptor
    /// index used by `AddressIndex::New` and `AddressIndex::LastUsed`.
    /// Use with caution, if an index is given that is less than the current descriptor index
    /// then the returned address may have already been used.
    Peek { index: u32 },
}

impl From<AddressIndex> for BdkAddressIndex {
    fn from(address_index: AddressIndex) -> Self {
        match address_index {
            AddressIndex::New => BdkAddressIndex::New,
            AddressIndex::LastUnused => BdkAddressIndex::LastUnused,
            AddressIndex::Peek { index } => BdkAddressIndex::Peek(index),
        }
    }
}

// TODO 9: Peek is not correctly implemented
impl From<&AddressIndex> for BdkAddressIndex {
    fn from(address_index: &AddressIndex) -> Self {
        match address_index {
            AddressIndex::New => BdkAddressIndex::New,
            AddressIndex::LastUnused => BdkAddressIndex::LastUnused,
            AddressIndex::Peek { index } => BdkAddressIndex::Peek(*index),
        }
    }
}

impl From<BdkAddressIndex> for AddressIndex {
    fn from(address_index: BdkAddressIndex) -> Self {
        match address_index {
            BdkAddressIndex::New => AddressIndex::New,
            BdkAddressIndex::LastUnused => AddressIndex::LastUnused,
            _ => panic!("Mmmm not working"),
        }
    }
}

impl From<&BdkAddressIndex> for AddressIndex {
    fn from(address_index: &BdkAddressIndex) -> Self {
        match address_index {
            BdkAddressIndex::New => AddressIndex::New,
            BdkAddressIndex::LastUnused => AddressIndex::LastUnused,
            _ => panic!("Mmmm not working"),
        }
    }
}

// /// A wallet transaction
// #[derive(Debug, Clone, PartialEq, Eq, Default)]
// pub struct TransactionDetails {
//     pub transaction: Option<Arc<Transaction>>,
//     /// Transaction id.
//     pub txid: String,
//     /// Received value (sats)
//     /// Sum of owned outputs of this transaction.
//     pub received: u64,
//     /// Sent value (sats)
//     /// Sum of owned inputs of this transaction.
//     pub sent: u64,
//     /// Fee value (sats) if confirmed.
//     /// The availability of the fee depends on the backend. It's never None with an Electrum
//     /// Server backend, but it could be None with a Bitcoin RPC node without txindex that receive
//     /// funds while offline.
//     pub fee: Option<u64>,
//     /// If the transaction is confirmed, contains height and timestamp of the block containing the
//     /// transaction, unconfirmed transaction contains `None`.
//     pub confirmation_time: Option<BlockTime>,
// }

//
// impl From<BdkTransactionDetails> for TransactionDetails {
//     fn from(tx_details: BdkTransactionDetails) -> Self {
//         let optional_tx: Option<Arc<Transaction>> =
//             tx_details.transaction.map(|tx| Arc::new(tx.into()));
//
//         TransactionDetails {
//             transaction: optional_tx,
//             fee: tx_details.fee,
//             txid: tx_details.txid.to_string(),
//             received: tx_details.received,
//             sent: tx_details.sent,
//             confirmation_time: tx_details.confirmation_time,
//         }
//     }
// }
//
// /// A reference to a transaction output.
// #[derive(Clone, Debug, PartialEq, Eq, Hash)]
// pub struct OutPoint {
//     /// The referenced transaction's txid.
//     txid: String,
//     /// The index of the referenced output in its transaction's vout.
//     vout: u32,
// }
//
// impl From<&OutPoint> for BdkOutPoint {
//     fn from(outpoint: &OutPoint) -> Self {
//         BdkOutPoint {
//             txid: Txid::from_str(&outpoint.txid).unwrap(),
//             vout: outpoint.vout,
//         }
//     }
// }

pub struct Balance {
    pub inner: BdkBalance,
}

impl Balance {
    /// All coinbase outputs not yet matured.
    fn immature(&self) -> u64 {
        self.inner.immature
    }

    /// Unconfirmed UTXOs generated by a wallet tx.
    fn trusted_pending(&self) -> u64 {
        self.inner.trusted_pending
    }

    /// Unconfirmed UTXOs received from an external wallet.
    fn untrusted_pending(&self) -> u64 {
        self.inner.untrusted_pending
    }

    /// Confirmed and immediately spendable balance.
    fn confirmed(&self) -> u64 {
        self.inner.confirmed
    }

    /// Get sum of trusted_pending and confirmed coins.
    fn trusted_spendable(&self) -> u64 {
        self.inner.trusted_spendable()
    }

    /// Get the whole balance visible to the wallet.
    fn total(&self) -> u64 {
        self.inner.total()
    }
}

// impl From<BdkBalance> for Balance {
//     fn from(bdk_balance: BdkBalance) -> Self {
//         Balance { inner: bdk_balance }
//     }
// }

// /// A transaction output, which defines new coins to be created from old ones.
// #[derive(Debug, Clone)]
// pub struct TxOut {
//     /// The value of the output, in satoshis.
//     value: u64,
//     /// The address of the output.
//     script_pubkey: Arc<Script>,
// }
//
// impl From<&BdkTxOut> for TxOut {
//     fn from(tx_out: &BdkTxOut) -> Self {
//         TxOut {
//             value: tx_out.value,
//             script_pubkey: Arc::new(Script {
//                 inner: tx_out.script_pubkey.clone(),
//             }),
//         }
//     }
// }
//
// pub struct LocalUtxo {
//     outpoint: OutPoint,
//     txout: TxOut,
//     keychain: KeychainKind,
//     is_spent: bool,
// }
//
// impl From<BdkLocalUtxo> for LocalUtxo {
//     fn from(local_utxo: BdkLocalUtxo) -> Self {
//         LocalUtxo {
//             outpoint: OutPoint {
//                 txid: local_utxo.outpoint.txid.to_string(),
//                 vout: local_utxo.outpoint.vout,
//             },
//             txout: TxOut {
//                 value: local_utxo.txout.value,
//                 script_pubkey: Arc::new(Script {
//                     inner: local_utxo.txout.script_pubkey,
//                 }),
//             },
//             keychain: local_utxo.keychain,
//             is_spent: local_utxo.is_spent,
//         }
//     }
// }
//
// /// Trait that logs at level INFO every update received (if any).
// pub trait Progress: Send + Sync + 'static {
//     /// Send a new progress update. The progress value should be in the range 0.0 - 100.0, and the message value is an
//     /// optional text message that can be displayed to the user.
//     fn update(&self, progress: f32, message: Option<String>);
// }
//
// struct ProgressHolder {
//     progress: Box<dyn Progress>,
// }
//
// impl BdkProgress for ProgressHolder {
//     fn update(&self, progress: f32, message: Option<String>) -> Result<(), BdkError> {
//         self.progress.update(progress, message);
//         Ok(())
//     }
// }
//
// impl Debug for ProgressHolder {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         f.debug_struct("ProgressHolder").finish_non_exhaustive()
//     }
// }
//
// #[derive(Debug, Clone)]
// pub struct TxIn {
//     pub previous_output: OutPoint,
//     pub script_sig: Arc<Script>,
//     pub sequence: u32,
//     pub witness: Vec<Vec<u8>>,
// }
//
// impl From<&BdkTxIn> for TxIn {
//     fn from(tx_in: &BdkTxIn) -> Self {
//         TxIn {
//             previous_output: OutPoint {
//                 txid: tx_in.previous_output.txid.to_string(),
//                 vout: tx_in.previous_output.vout,
//             },
//             script_sig: Arc::new(Script {
//                 inner: tx_in.script_sig.clone(),
//             }),
//             sequence: tx_in.sequence.0,
//             witness: tx_in.witness.to_vec(),
//         }
//     }
// }

// /// The method used to produce an address.
// #[derive(Debug)]
// pub enum Payload {
//     /// P2PKH address.
//     PubkeyHash { pubkey_hash: Vec<u8> },
//     /// P2SH address.
//     ScriptHash { script_hash: Vec<u8> },
//     /// Segwit address.
//     WitnessProgram {
//         /// The witness program version.
//         version: WitnessVersion,
//         /// The witness program.
//         program: Vec<u8>,
//     },
// }

// impl From<BdkScript> for Script {
//     fn from(bdk_script: BdkScript) -> Self {
//         Script { inner: bdk_script }
//     }
// }
//
// #[derive(Clone, Debug)]
// enum RbfValue {
//     Default,
//     Value(u32),
// }
//
// /// The result after calling the TxBuilder finish() function. Contains unsigned PSBT and
// /// transaction details.
// pub struct TxBuilderResult {
//     pub(crate) psbt: Arc<PartiallySignedTransaction>,
//     pub transaction_details: TransactionDetails,
// }
//
// uniffi::deps::static_assertions::assert_impl_all!(Wallet: Sync, Send);
//
// // The goal of these tests to to ensure `bdk-ffi` intermediate code correctly calls `bdk` APIs.
// // These tests should not be used to verify `bdk` behavior that is already tested in the `bdk`
// // crate.
// #[cfg(test)]
// mod test {
//     use super::Transaction;
//     use crate::Network::Regtest;
//     use crate::{Address, Payload};
//     use assert_matches::assert_matches;
//     use bdk::bitcoin::hashes::hex::FromHex;
//     use bdk::bitcoin::util::address::WitnessVersion;
//
//     // Verify that bdk-ffi Transaction can be created from valid bytes and serialized back into the same bytes.
//     #[test]
//     fn test_transaction_serde() {
//         let test_tx_bytes = Vec::from_hex("020000000001031cfbc8f54fbfa4a33a30068841371f80dbfe166211242213188428f437445c91000000006a47304402206fbcec8d2d2e740d824d3d36cc345b37d9f65d665a99f5bd5c9e8d42270a03a8022013959632492332200c2908459547bf8dbf97c65ab1a28dec377d6f1d41d3d63e012103d7279dfb90ce17fe139ba60a7c41ddf605b25e1c07a4ddcb9dfef4e7d6710f48feffffff476222484f5e35b3f0e43f65fc76e21d8be7818dd6a989c160b1e5039b7835fc00000000171600140914414d3c94af70ac7e25407b0689e0baa10c77feffffffa83d954a62568bbc99cc644c62eb7383d7c2a2563041a0aeb891a6a4055895570000000017160014795d04cc2d4f31480d9a3710993fbd80d04301dffeffffff06fef72f000000000017a91476fd7035cd26f1a32a5ab979e056713aac25796887a5000f00000000001976a914b8332d502a529571c6af4be66399cd33379071c588ac3fda0500000000001976a914fc1d692f8de10ae33295f090bea5fe49527d975c88ac522e1b00000000001976a914808406b54d1044c429ac54c0e189b0d8061667e088ac6eb68501000000001976a914dfab6085f3a8fb3e6710206a5a959313c5618f4d88acbba20000000000001976a914eb3026552d7e3f3073457d0bee5d4757de48160d88ac0002483045022100bee24b63212939d33d513e767bc79300051f7a0d433c3fcf1e0e3bf03b9eb1d70220588dc45a9ce3a939103b4459ce47500b64e23ab118dfc03c9caa7d6bfc32b9c601210354fd80328da0f9ae6eef2b3a81f74f9a6f66761fadf96f1d1d22b1fd6845876402483045022100e29c7e3a5efc10da6269e5fc20b6a1cb8beb92130cc52c67e46ef40aaa5cac5f0220644dd1b049727d991aece98a105563416e10a5ac4221abac7d16931842d5c322012103960b87412d6e169f30e12106bdf70122aabb9eb61f455518322a18b920a4dfa887d30700").unwrap();
//         let new_tx_from_bytes = Transaction::new(test_tx_bytes.clone()).unwrap();
//         let serialized_tx_to_bytes = new_tx_from_bytes.serialize();
//         assert_eq!(test_tx_bytes, serialized_tx_to_bytes);
//     }
//
//     // Verify that bdk-ffi Address.payload includes expected WitnessProgram variant, version and program bytes.
//     #[test]
//     fn test_address_witness_program() {
//         let address =
//             Address::new("bcrt1qqjn9gky9mkrm3c28e5e87t5akd3twg6xezp0tv".to_string()).unwrap();
//         let payload = address.payload();
//         assert_matches!(payload, Payload::WitnessProgram { version, program } => {
//             assert_eq!(version,WitnessVersion::V0);
//             assert_eq!(program, Vec::from_hex("04a6545885dd87b8e147cd327f2e9db362b72346").unwrap());
//         });
//         assert_eq!(address.network(), Regtest);
//     }
// }
