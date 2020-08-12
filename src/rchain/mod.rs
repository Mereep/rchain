extern crate blake2;

use std::vec::Vec;
use std::collections::HashMap;
use std::convert::Into;
use std::time::SystemTime;
use blake2::{Blake2b, Digest};
use std::string::String;
use std::convert::From;


/// The actual Blockchain container
#[derive(Debug, Clone)]
pub struct Blockchain {
    /// Stores all the blocks which are accepted already within the blockchain
    pub blocks: Vec<Block>,

    /// Lookup from AccountID (will be a public key later) to Account.
    /// Effectively, this represents the WorldState
    pub accounts: HashMap<String, Account>,

    /// Will store transactions which should be added to the chain
    /// but aren't yet
    pending_transactions: Vec<Transaction>,
}

/// Represents the current state of the blockchain after all Blocks are executed
/// A world state is technically not necessary since we always could build the information
/// by iterating through all the blocks. Generally, this doesn't seem like a good option
/// However, we do not force the actual Blockchain to implement a WorldState but rather
/// behave like having one. This trait therefore just defines an expected interface into our Blockchain
/// (Actually it doesn't even care if we the information is stored within a blockchain)
pub trait WorldState {
    /// Will bring us all registered user ids
    fn get_user_ids(&self) -> Vec<String>;

    /// Will return an account given it's id if is available (mutable)
    fn get_account_by_id_mut(&mut self, id: &String) -> Option<&mut Account>;

    /// Will return an account given it's id if is available
    fn get_account_by_id(&self, id: &String) -> Option<&Account>;

    /// Will add a new account
    fn create_account(&mut self, id: String, account_type: AccountType) -> Result<(), &'static str>;
}

/// One single part of the blockchain.
/// Basically contains a list of transactions
#[derive(Clone, Debug)]
pub struct Block {
    /// Actions that this block includes
    /// There has to be at least one
    pub(crate) transactions: Vec<Transaction>,

    /// This actually connects the blocks together
    prev_hash: Option<String>,

    /// We store the hash of the block here also in order to
    /// save the last block from being tampered with later on
    hash: Option<String>,

    /// Some arbitrary number which will be later used for Proof of Work
    nonce: u128,
}

/// Stores a request to the blockchain
#[derive(Clone, Debug)]
pub struct Transaction {
    /// Unique number (will be used for randomization later; prevents replay attacks)
    nonce: u128,

    /// Account ID
    from: String,

    /// Stores the time the transaction was created
    created_at: SystemTime,

    /// the type of the transaction and its additional information
    pub(crate) record: TransactionData,

    /// Signature of the hash of the whole message
    signature: Option<String>,
}

/// A single operation to be stored on the chain
/// Noticeable, enums in rust actually can carry data in a
/// tuple-like structure (CreateUserAccount) or a dictionary-like (the ChangeStoreValue)
#[derive(Clone, Debug, PartialEq)]
pub enum TransactionData {
    /// Will be used to store a new user account
    CreateUserAccount(String),

    /// Will be used to change or create a arbitrary value into an account
    ChangeStoreValue { key: String, value: String },

    /// Will be used to move tokens from one owner to another
    TransferTokens { to: String, amount: u128 },

    /// Just create tokens out of nowhere
    CreateTokens { receiver: String, amount: u128 },

    // ... Extend it as you wish, you get the idea
}

/// Represents an account on the blockchain
/// This is basically the primary part of the "world state" of the blockchain
/// It is the final status after performing all blocks in order
#[derive(Clone, Debug)]
pub struct Account {
    /// We want the account to be able to store any information we want (Dictionary)
    store: HashMap<String, String>,

    /// store if this is a user account or sth else
    acc_type: AccountType,

    /// Amount of tokens that account owns (like BTC or ETH)
    tokens: u128,
}

/// We can support different types of accounts
/// which could be used to represent different roles within the system
/// This is just for later extension, for now we will only use User accounts
#[derive(Clone, Debug)]
pub enum AccountType {
    /// A common user account
    User,

    /// An account that technically does not represent an individual
    /// Think of this like a SmartContract in Ethereum. We will not use it
    /// in our implementation. It's just here if you want to go on implementing
    /// to provide a starting point for more :)
    Contract,

    /// Add whatever roles you need.
    /// Again, we will NOT make use of this for the example here
    Validator {
        // Again, enum members in rust may store additional data
        correctly_validated_blocks: u128,
        incorrectly_validated_blocks: u128,
        you_get_the_idea: bool,
    },
}

impl Blockchain {
    /// Constructor
    pub fn new() -> Self {
        Blockchain {
            blocks: Vec::new(),
            accounts: HashMap::new(),
            pending_transactions: Vec::new(),
        }
    }

    /// Will add a block to the Blockchain
    /// @TODO every simple step could be refactored into a separate function for
    /// better testability and code-reusability
    pub fn append_block(&mut self, block: Block) -> Result<(), String> {

        // The genesis block may create user out of nowhere,
        // and also may do some other things
        let is_genesis = self.len() == 0;

        // Check if the hash matches the transactions
        if !block.verify_own_hash() {
            return Err("The block hash is mismatching! (Code: 93820394)".into());
        }

        // Check if the newly added block is meant to be appended onto the last block
        if !(block.prev_hash == self.get_last_block_hash()) {
            return Err("The new block has to point to the previous block (Code: 3948230)".into());
        }

        // There has to be at least one transaction inside the queue
        if block.get_transaction_count() == 0 {
            return Err("There has to be at least one transaction \
            inside the block! (Code: 9482930)".into());
        }

        // Reject block having nonces that are already used (Prevent reply attacks etc.)
        // @Todo (Will skip that for simplicity)


        // This is expensive and just used for rollback if some transactions succeed whilst
        // others don't (prevent inconsistent states)
        // Arguably, that could be implemented more resource-aware
        let old_state = self.accounts.clone();

        // Execute each transaction
        for (i, transaction) in block.transactions.iter().enumerate() {

            // Execute the transaction
            if let Err(err) = transaction.execute(self, &is_genesis) {
                // Recover state on failure
                self.accounts = old_state;

                // ... and reject the block
                return Err(format!("Could not execute transaction {} due to `{}`. Rolling back \
                (Code: 38203984)", i + 1, err));
            }
        }

        // Everything went fine... append the block
        self.blocks.push(block);

        Ok(())
    }

    /// Will return the amount of blocks currently stored
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Will return the hash of the last block
    pub fn get_last_block_hash(&self) -> Option<String> {
        if self.len() == 0 {
            return None;
        }

        self.blocks[self.len() - 1].hash.clone()
    }

    /// Checks if the blockchain was tempered with
    /// It will check until the first error happens and return a description of the problem
    /// if everything is fine it will return Ok
    pub fn check_validity(&self) -> Result<(), String> {
        for (block_num, block) in self.blocks.iter().enumerate() {

            // Check if block saved hash matches to calculated hash
            if !block.verify_own_hash() {
                return Err(format!("Stored hash for Block #{} \
                    does not match calculated hash (Code: 665234234)", block_num + 1).into());
            }

            // Check previous black hash points to actual previous block
            if block_num == 0 {
                // Genesis block should point to nowhere
                if block.prev_hash.is_some() {
                    return Err("The genesis block has a previous hash set which \
                     it shouldn't Code :394823098".into());
                }
            } else {
                // Non genesis blocks should point to previous blocks hash (which is validated before)
                if block.prev_hash.is_none() {
                    return Err(format!("Block #{} has no previous hash set", block_num + 1).into());
                }

                // Store the values locally to use them within the error message on failure
                let prev_hash_proposed = block.prev_hash.as_ref().unwrap();
                let prev_hash_actual = self.blocks[block_num - 1].hash.as_ref().unwrap();

                if !(&block.prev_hash == &self.blocks[block_num - 1].hash) {
                    return Err(format!("Block #{} is not connected to previous block (Hashes do \
                    not match. Should be `{}` but is `{}`)", block_num, prev_hash_proposed,
                                       prev_hash_actual).into());
                }
            }

            // Check if transactions are signed correctly
            for (transaction_num, transaction) in block.transactions.iter().enumerate() {

                // Careful! With that implementation an unsigned message will always
                // be valid! You may remove the first check to only accept signed transactions
                if transaction.is_signed() && !transaction.check_signature() {
                    return Err(format!("Transaction #{} for Block #{} has an invalid signature \
                    (Code: 4398239048)", transaction_num + 1, block_num + 1));
                }
            }
        }
        Ok(())
    }
}

impl WorldState for Blockchain {
    fn get_user_ids(&self) -> Vec<String> {
        self.accounts.keys().map(|s| s.clone()).collect()
    }

    fn get_account_by_id_mut(&mut self, id: &String) -> Option<&mut Account> {
        self.accounts.get_mut(id)
    }

    fn get_account_by_id(&self, id: &String) -> Option<&Account> {
        self.accounts.get(id)
    }

    fn create_account(&mut self, id: String,
                      account_type: AccountType) -> Result<(), &'static str> {
        return if !self.get_user_ids().contains(&id) {
            let acc = Account::new(account_type);
            self.accounts.insert(id, acc);
            Ok(())
        } else {
            Err("User already exists! (Code: 934823094)")
        };
    }
}

impl Block {
    pub fn new(prev_hash: Option<String>) -> Self {
        Block {
            nonce: 0,
            hash: None,
            prev_hash,
            transactions: Vec::new(),
        }
    }

    /// Changes the nonce number and updates the hash
    pub fn set_nonce(&mut self, nonce: u128) {
        self.nonce = nonce;
        self.update_hash();
    }

    /// Will calculate the hash of the whole block including transactions Blake2 hasher
    pub fn calculate_hash(&self) -> Vec<u8> {
        let mut hasher = Blake2b::new();

        for transaction in self.transactions.iter() {
            hasher.update(transaction.calculate_hash())
        }

        let block_as_string = format!("{:?}", (&self.prev_hash, &self.nonce));
        hasher.update(&block_as_string);

        return Vec::from(hasher.finalize().as_ref());
    }

    /// Appends a transaction to the queue
    pub fn add_transaction(&mut self, transaction: Transaction) {
        self.transactions.push(transaction);
        self.update_hash();
    }

    /// Will return the amount of transactions
    pub fn get_transaction_count(&self) -> usize {
        self.transactions.len()
    }

    /// Will update the hash field by including all transactions currently inside
    /// the public modifier is only for the demonstration of attacks
    pub(crate) fn update_hash(&mut self) {
        self.hash = Some(byte_vector_to_string(&self.calculate_hash()));
    }

    /// Checks if the hash is set and matches the blocks interna
    pub fn verify_own_hash(&self) -> bool {
        if self.hash.is_some() && // Hash set
            self.hash.as_ref().unwrap().eq(
                &byte_vector_to_string(
                    &self.calculate_hash())) { // Hash equals calculated hash

            return true;
        }
        false
    }
}

impl Transaction {
    pub fn new(from: String, transaction_data: TransactionData, nonce: u128) -> Self {
        Transaction {
            from,
            nonce,
            record: transaction_data,
            created_at: SystemTime::now(),
            signature: None,
        }
    }

    /// Will change the world state according to the transactions commands
    pub fn execute<T: WorldState>(&self, world_state: &mut T, is_initial: &bool) -> Result<(), &'static str> {
        // Check if sending user does exist (no one not on the chain can execute transactions)
        if let Some(_account) = world_state.get_account_by_id(&self.from) {
            // Do some more checkups later on...
        } else {
            if !is_initial {
                return Err("Account does not exist (Code: 93482390)");
            }
        }

        // match is like a switch (pattern matching) in C++ or Java
        // We will check for the type of transaction here and execute its logic
        return match &self.record {

            TransactionData::CreateUserAccount(account) => {
                world_state.create_account(account.into(), AccountType::User)
            }

            TransactionData::CreateTokens { receiver, amount } => {
                if !is_initial {
                    return Err("Token creation is only available on initial creation (Code: 2394233)");
                }
                // Get the receiving user (must exist)
                return if let Some(account) = world_state.get_account_by_id_mut(receiver) {
                    account.tokens += *amount;
                    Ok(())
                } else {
                    Err("Receiver Account does not exist (Code: 23482309)")
                };
            }

            TransactionData::TransferTokens { to, amount } => {
                let recv_tokens: u128;
                let sender_tokens: u128;

                if let Some(recv) = world_state.get_account_by_id_mut(to) {
                    // Be extra careful here, even in the genesis block the sender account has to exist
                    recv_tokens = recv.tokens;
                } else {
                    return Err("Receiver Account does not exist! (Code: 3242342380)");
                }

                if let Some(sender) = world_state.get_account_by_id_mut(&self.from) {
                    sender_tokens = sender.tokens;
                } else {
                    return Err("That account does not exist! (Code: 23423923)");
                }

                let balance_recv_new = recv_tokens.checked_add(*amount);
                let balance_sender_new = sender_tokens.checked_sub(*amount);

                if balance_recv_new.is_some() && balance_sender_new.is_some() {
                    world_state.get_account_by_id_mut(&self.from).unwrap().tokens = balance_sender_new.unwrap();
                    world_state.get_account_by_id_mut(to).unwrap().tokens = balance_recv_new.unwrap();
                    return Ok(());
                } else {
                    return Err("Overspent or Arithmetic error (Code: 48239084203)");
                }
            }

            _ => { // Not implemented transaction type
                Err("Unknown Transaction type (not implemented) (Code: 487289724389)")
            }
        };
    }

    /// Will calculate the hash using Blake2 hasher
    pub fn calculate_hash(&self) -> Vec<u8> {
        let mut hasher = Blake2b::new();
        let transaction_as_string = format!("{:?}", (&self.created_at, &self.record,
                                                     &self.from, &self.nonce));

        hasher.update(&transaction_as_string);
        return Vec::from(hasher.finalize().as_ref());
    }

    /// Will hash the transaction and check if the signature is valid
    /// (i.e., it is created by the owners private key)
    /// if the message is not signed it will always return false
    pub fn check_signature(&self) -> bool {
        if !(self.is_signed()) {
            return false;
        }

        //@TODO check signature
        false
    }

    pub fn is_signed(&self) -> bool {
        self.signature.is_some()
    }
}

impl Account {
    /// Constructor
    pub fn new(account_type: AccountType) -> Self {
        return Self {
            tokens: 0,
            acc_type: account_type,
            store: HashMap::new(),
        };
    }
}

/// Will take an array of bytes and transform it into a string by interpreting every byte
/// as an character due to RFC 1023 that's not possible
/// @Link https://github.com/rust-lang/rfcs/blob/master/text/1023-rebalancing-coherence.md
/// (trait and parameters are not within the local crate)
/*impl From<&std::vec::Vec<u8>> for std::string::String {
    fn from(item: &Vec<u8>) -> Self {
        item.iter().map(|&c| c as char).collect()
    }
}*/

/// Will take an array of bytes and transform it into a string by interpreting every byte
/// as an character
fn byte_vector_to_string(arr: &Vec<u8>) -> String {
    arr.iter().map(|&c| c as char).collect()
}
