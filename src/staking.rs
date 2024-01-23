use crate::*;
use near_sdk::json_types::U128;
use near_sdk::{assert_one_yocto, env};
use crate::utils::{U256, nano_to_sec, REWARD_PER_SECOND, assert_parent_contract, convert_to_yocto};


impl Contract {
    fn reward_per_token(&self) -> u128 {
        if self.total_supply == 0 {
            return self.reward_per_token_stored;
        }
        let seconds_diff = nano_to_sec(env::block_timestamp() - self.last_update_time);

        let reward = U256::from(seconds_diff) * U256::from(REWARD_PER_SECOND) * U256::from(ONE_TOKEN);
        self.reward_per_token_stored + (reward / U256::from(self.total_supply)).as_u128()
    }

    fn earned(&self, account_id: &AccountId) -> u128 {
        let user_reward = self.rewards.get(account_id).unwrap_or(0);
        if self.reward_per_token() > 0 {
            let user_balance = self.balances.get(account_id).unwrap_or(0);
            let user_reward_per_token_paid = self.user_reward_per_token_paid.get(account_id).unwrap_or(0);
            let reward_diff = self.reward_per_token() - user_reward_per_token_paid;
            let mut earn_current = (U256::from(user_balance) * U256::from(reward_diff) / U256::from(ONE_TOKEN)).as_u128();

            let monster_reward = self.stake_monster_pct.get(account_id).unwrap_or(0);
            if monster_reward > 0 {
                earn_current = earn_current + (earn_current / 100) * monster_reward as u128;
            }
            return user_reward + earn_current;
        }
        user_reward
    }

    fn update_reward(&mut self, account_id: &AccountId) {
        self.reward_per_token_stored = self.reward_per_token();
        self.last_update_time = env::block_timestamp();
        self.rewards.insert(account_id, &self.earned(account_id));
        self.user_reward_per_token_paid.insert(account_id, &self.reward_per_token_stored);
    }
}

#[near_bindgen]
impl Contract {
    pub fn internal_stake(&mut self, account_id: &AccountId, amount: U128) {
        let amount = amount.0;
        assert!(amount > 0, "Please specify staking amount");

        self.update_reward(account_id);

        let mut user_balance = self.balances.get(account_id).unwrap_or(0);
        user_balance += amount;
        self.balances.insert(account_id, &user_balance);
        self.total_supply += amount;
    }

    #[payable]
    pub fn withdraw_stake(&mut self, amount: U128) {
        assert_one_yocto();
        let mut amount = amount.0;
        let account_id = env::predecessor_account_id();
        self.update_reward(&account_id);

        let mut user_balance = self.balances.get(&account_id).unwrap_or(0);

        if user_balance < amount {
            amount = user_balance;
        }

        user_balance -= amount;
        self.balances.insert(&account_id, &user_balance);
        self.total_supply -= amount;

        self.token.internal_deposit(&account_id, amount);
    }

    #[payable]
    pub fn withdraw_reward(&mut self) {
        if env::attached_deposit() < convert_to_yocto("0.1") {
            env::panic_str("Attach claim deposit!");
        }

        let account_id = env::predecessor_account_id();
        self.update_reward(&account_id);

        let reward = self.rewards.get(&account_id).unwrap_or(0);
        if reward < 1 {
            env::panic_str("You don't have rewards");
        }

        self.rewards.insert(&account_id, &0);
        self.token.internal_deposit(&account_id, reward);
    }

    pub fn stake_monster(&mut self, bonus_pct: u8, account_id: AccountId) {
        assert_parent_contract();
        self.update_reward(&account_id);
        self.stake_monster_pct.insert(&account_id, &bonus_pct);
    }

    pub fn unstake_monster(&mut self, account_id: AccountId) {
        assert_parent_contract();
        self.update_reward(&account_id);
        self.stake_monster_pct.remove(&account_id);
    }

    pub fn get_total_supply(&self) -> U128 {
        self.total_supply.into()
    }

    pub fn get_user_stake(&self, account_id: AccountId) -> U128 {
        let user_stake = self.balances.get(&account_id).unwrap_or(0);
        user_stake.into()
    }

    pub fn get_user_earned(&self, account_id: AccountId) -> U128 {
        self.earned(&account_id).into()
    }

    pub fn get_apr(&self) -> U128 {
        if self.total_supply > 0 {
            let year_seconds = 60 * 60 * 24 * 365;
            let apr = (U256::from(REWARD_PER_SECOND) * U256::from(year_seconds) * U256::from(100) / U256::from(self.total_supply)).as_u128();
            return apr.into();
        }
        0.into()
    }

    pub fn get_reward_per_token(&self) -> U128 {
        self.reward_per_token().into()
    }

    pub fn get_stake_monster_pct(&self, account_id: AccountId) -> u8 {
        self.stake_monster_pct.get(&account_id).unwrap_or(0)
    }
}
