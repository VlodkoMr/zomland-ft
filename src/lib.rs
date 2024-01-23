/*!
Fungible Token implementation with JSON serialization.
NOTES:
  - The maximum balance value is limited by U128 (2**128 - 1).
  - JSON calls should pass U128 as a base-10 string. E.g. "100".
  - The contract optimizes the inner trie structure by hashing account IDs. It will prevent some
    abuse of deep tries. Shouldn't be an issue, once NEAR clients implement full hashing of keys.
  - The contract tracks the change in storage before and after the call. If the storage increases,
    the contract requires the caller of the contract to attach enough deposit to the function call
    to cover the storage cost.
    This is done to prevent a denial of service attack on the contract by taking all available storage.
    If the storage decreases, the contract will issue a refund for the cost of the released storage.
    The unused tokens from the attached deposit are also refunded, so it's safe to
    attach more deposit than required.
  - To prevent the deployed contract from being modified or deleted, it should not have any access
    keys on its account.
 */

use near_contract_standards::fungible_token::metadata::{
    FT_METADATA_SPEC, FungibleTokenMetadata, FungibleTokenMetadataProvider,
};
use near_contract_standards::fungible_token::FungibleToken;
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::{AccountId, Balance, env, log, near_bindgen, PanicOnDefault, Promise, PromiseOrValue, BorshStorageKey};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, LookupMap};
use near_sdk::json_types::{U128};
use std::convert::TryInto;
use crate::utils::{ONE_TOKEN, assert_parent_contract};

mod utils;
mod staking;

const FT_IMAGE_ICON: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAABABAMAAABYR2ztAAAAJ1BMVEUAAADuw0fNfR3quUHShyPgpDXlsDvXkSn126/cmy/xyH7wxVzxu1mK0HLnAAAAAXRSTlMAQObYZgAAA2dJREFUSMd1VT1PG0EQJUqDnBS8nO/OBpocZ58oFxtFlD4TUEobAUp5hECNFROlM0qDEho7X0oHSoNCA0oTUYU6fypvZ3d9Bw4PIdmet2/ezOzH1B0cHw+m7sX0xa8GAHV9D+cDxjj/X/wjAO/15pctBaAyGf8BBNtpRMT9DN7sxPorLEtYKBvA79vxErAiofVVoX0CTm8R/sLnz+vMTx9v+HGIuWItJwi48FDBYJ9SWTHJ9BW6IuvAdDVgUBCocj0KeBZFB6gUBHpcoRG8/OqBoGKSuygh1DkJOolX17dAqykl/ljCGUZRW//4NjVt2JMklLAZWGKsdCMjiw0QaTScM714jB0RWIoc+prRjOqYMRlIzmSJRRpv87sXxTATUb6UEJoobXrp80wKGUorStQ+ALBjDIZRwh9eZXrFggzkAUaSoWcISGsoi6QXLaIiFqJEvhpkuzVU428AuFBpEzc+pYCqI3iL8GM/UyyLJnQXysaCI+AzgiRoiYkWBuKx4ywQHewiqKOtO8dOnLJN3VjlXaBaMwueog+CfmdI6CXICVxUPgzaMC4TEh4ilZocIVGh8to0AlCbdb73uCgvgrVX4dXxDmKcdZ4FUmWYE3x4NbyAjC/ThHyU9RFTIIDXwhoANqAzp/vUcoQWa41J8DvegSXAEcpSohA8JgmEEBYIVSGMZLtTI2zdVfAi1jkkrQ2N5kZOYBWuUZ2AKkJg3Jp0ZYLq5Kf813oKtsy5qSM2Sr4YAudi4Rp1hHTROKYH+AkcXKsf2mH5Mkl8LxIYqehxR0rKkAoagIKkUXbcbsPQnz2gms0/s2EuueVCStsyasjhthw7RaLblCJhCxHpgZw8cVneNzY9yWG3vT7fR/bg+L5sOBdW44PzCE3msnWQ6iBH79JcD6S6bVm3HvLDS5fomYVp3LPDhJyKGGBYm1gy8+rVnkTK+LMXSEUIJXsFYbTgH9r1krAjFiSHucS60rCxQIL58UsRiv2dTh4P0sJNWmJ6XcjKeAxKLtL8wj/Te3YNBezrps4WX4uuu6zzy1wEnIR0qa/ssOU5cAJOQk8i2WoADfegUCDHCWWpwavcPUmuhDwJlnvFR212cOfVvWFu9ywqYN7Fi88SKXubm3tiVeKTDAd5ayYZ48f752TQUS6A6/Nby/8BNBAG258AcBEAAAAASUVORK5CYII=";

#[derive(BorshStorageKey, BorshSerialize)]
pub enum StorageKeys {
    Token,
    UserRewardPerTokenPaid,
    Rewards,
    Balances,
    StakeMonsterPct,
    ZmlReserved,
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct OldContract {
    token: FungibleToken,
    metadata: LazyOption<FungibleTokenMetadata>,
    owner_id: AccountId,
    user_reward_per_token_paid: LookupMap<AccountId, u128>,
    rewards: LookupMap<AccountId, u128>,
    balances: LookupMap<AccountId, u128>,
    stake_monster_pct: LookupMap<AccountId, u8>,
    zml_reserved: LookupMap<AccountId, Balance>,
    total_supply: u128,
    last_update_time: u64,
    reward_per_token_stored: u128,
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    token: FungibleToken,
    metadata: LazyOption<FungibleTokenMetadata>,
    owner_id: AccountId,
    user_reward_per_token_paid: LookupMap<AccountId, u128>,
    rewards: LookupMap<AccountId, u128>,
    balances: LookupMap<AccountId, u128>,
    stake_monster_pct: LookupMap<AccountId, u8>,
    zml_reserved: LookupMap<AccountId, Balance>,
    total_supply: u128,
    last_update_time: u64,
    reward_per_token_stored: u128,
}

#[near_bindgen]
impl Contract {
    // #[private]
    // #[init(ignore_state)]
    // pub fn migrate() -> Self {
    //     let old_state: OldContract = env::state_read().expect("failed");
    //
    //     Self {
    //         token: old_state.token,
    //         metadata: old_state.metadata,
    //         owner_id: old_state.owner_id,
    //         user_reward_per_token_paid: old_state.user_reward_per_token_paid,
    //         rewards: old_state.rewards,
    //         balances: old_state.balances,
    //         stake_monster_pct: old_state.stake_monster_pct,
    //         zml_reserved: old_state.zml_reserved,
    //         total_supply: old_state.total_supply,
    //         last_update_time: old_state.last_update_time,
    //         reward_per_token_stored: old_state.reward_per_token_stored,
    //     }
    // }

    #[init]
    pub fn new_default_meta(owner_id: AccountId, total_supply: U128) -> Self {
        Self::new(
            owner_id,
            total_supply,
            FungibleTokenMetadata {
                spec: FT_METADATA_SPEC.to_string(),
                name: "ZomLand Token".to_string(),
                symbol: "ZML".to_string(),
                icon: Some(FT_IMAGE_ICON.to_string()),
                reference: None,
                reference_hash: None,
                decimals: 24,
            },
        )
    }

    #[init]
    pub fn new(
        owner_id: AccountId,
        total_supply: U128,
        metadata: FungibleTokenMetadata,
    ) -> Self {
        assert!(!env::state_exists(), "Already initialized");
        metadata.assert_valid();

        let mut this = Self {
            owner_id,
            token: FungibleToken::new(StorageKeys::Token),
            metadata: LazyOption::new(b"m".to_vec(), Some(&metadata)),
            user_reward_per_token_paid: LookupMap::new(StorageKeys::UserRewardPerTokenPaid),
            rewards: LookupMap::new(StorageKeys::Rewards),
            balances: LookupMap::new(StorageKeys::Balances),
            stake_monster_pct: LookupMap::new(StorageKeys::StakeMonsterPct),
            zml_reserved: LookupMap::new(StorageKeys::ZmlReserved),
            total_supply: 0,
            last_update_time: env::block_timestamp(),
            reward_per_token_stored: 0,
        };

        // Leave 80 million tokens for staking in current contract
        // let staking_tokens = 80 * 1_000_000 * ONE_TOKEN;
        // let send_tokens = total_supply.0 - staking_tokens;

        let current_contract = env::current_account_id();
        // let (_, main_contract) = current_contract.split_once(".").unwrap();
        // let main_contract = String::from(main_contract);

        // this.token.internal_register_account(&main_contract);
        // this.token.internal_deposit(&main_contract, send_tokens);

        this.token.internal_register_account(&current_contract);
        this.token.internal_deposit(&current_contract, total_supply.0);

        this
    }

    fn on_account_closed(&mut self, account_id: AccountId, balance: Balance) {
        log!("Closed @{} with {}", account_id, balance);
    }

    fn on_tokens_burned(&mut self, account_id: AccountId, amount: Balance) {
        log!("Account @{} burned {}", account_id, amount);
    }

    #[payable]
    pub fn ft_mint(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
    ) {
        //get initial storage usage
        assert_eq!(amount.0, 0, "Cannot mint tokens, just 0 for approve");

        let initial_storage_usage = env::storage_usage();

        if !self.token.accounts.contains_key(&receiver_id) {
            self.token.accounts.insert(&receiver_id, &0);
        }

        //refund any excess storage
        let storage_used = env::storage_usage() - initial_storage_usage;
        let required_cost = env::storage_byte_cost() * Balance::from(storage_used);
        let attached_deposit = env::attached_deposit();

        assert!(
            required_cost <= attached_deposit,
            "Must attach {} yoctoNEAR to cover storage", required_cost
        );

        let refund = attached_deposit - required_cost;
        if refund > 1 {
            Promise::new(env::predecessor_account_id()).transfer(refund);
        }
    }

    pub(crate) fn add_zml_reserve(&mut self, account_id: &AccountId, amount: U128) {
        let amount = amount.0;
        assert!(amount > 0, "Please provide correct ZML Amount");

        let mut reserved = self.zml_reserved.get(account_id).unwrap_or(0);
        reserved += amount;
        self.zml_reserved.insert(account_id, &reserved);
    }

    pub fn get_zml_reserve(&self, account_id: &AccountId) -> U128 {
        self.zml_reserved.get(account_id).unwrap_or(0).into()
    }

    pub fn burn_zml_reserve(&mut self, account_id: &AccountId, required_zml: U128) -> U128 {
        let main_contract = assert_parent_contract();
        let required_zml = required_zml.0;
        let mut reserved = self.zml_reserved.get(account_id).unwrap_or(0);
        if reserved >= required_zml {
            reserved -= required_zml;
            self.zml_reserved.insert(account_id, &reserved);

            // transfer to burn
            let burn_account_id = format!("burn.{}", main_contract).try_into().unwrap();
            self.token.internal_deposit(&burn_account_id, required_zml);

            return required_zml.into();
        } else {
            env::panic_str("Not enough ZML reserve");
        }
    }

    pub fn transfer_zml_reserve(&mut self, sender_id: &AccountId, receiver_id: &AccountId, required_zml: U128) -> U128 {
        let main_contract = assert_parent_contract();
        let required_zml = required_zml.0;
        let mut reserved = self.zml_reserved.get(sender_id).unwrap_or(0);

        if reserved >= required_zml {
            reserved -= required_zml;
            self.zml_reserved.insert(sender_id, &reserved);

            let commission = 0.005;
            let tax = (required_zml as f64 * commission) as u128;
            let total = (required_zml - tax) as u128;

            // transfer to recipient account
            self.token.internal_deposit(receiver_id, total);
            self.token.internal_deposit(&main_contract, tax);

            return required_zml.into();
        } else {
            env::panic_str("Not enough ZML reserve");
        }
    }

    pub fn withdraw_zml_reserve(&mut self) {
        let account_id = env::predecessor_account_id();
        let reserved = self.zml_reserved.get(&account_id).unwrap_or(0);
        if reserved > 0 {
            self.zml_reserved.insert(&account_id, &0);
            self.token.internal_deposit(&account_id, reserved);
        } else {
            env::panic_str("No reserved ZML");
        }
    }
}

near_contract_standards::impl_fungible_token_core!(Contract, token, on_tokens_burned);
near_contract_standards::impl_fungible_token_storage!(Contract, token, on_account_closed);

#[near_bindgen]
impl FungibleTokenMetadataProvider for Contract {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.get().unwrap()
    }
}

#[near_bindgen]
impl FungibleTokenReceiver for Contract {
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        match &msg[..] {
            "ft_staking" => {
                self.internal_stake(&sender_id, amount.into());
                PromiseOrValue::Value(U128(0))
            }
            "ft_add_zml_reserve" => {
                self.add_zml_reserve(&sender_id, amount.into());
                PromiseOrValue::Value(U128(0))
            }
            "ft_create_user_clan" => {
                self.add_zml_reserve(&sender_id, amount.into());
                PromiseOrValue::Value(U128(0))
            }
            _ => {
                env::log_str("Invalid instruction for raffle call");
                PromiseOrValue::Value(amount)
            }
        }
    }
}

