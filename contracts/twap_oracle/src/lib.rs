#![no_std]
use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env};

#[cfg(test)]
mod test;

/// Errors returned by the `TwapOracle` contract.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidPrice = 3,
    InsufficientTimeElapsed = 4,
    /// A cumulative-price or elapsed-time accumulator would have overflowed.
    ///
    /// The TWAP accumulators are `u128`; a long-running feed with a large price
    /// can exhaust that range, and silently wrapping would corrupt every
    /// subsequent TWAP. The update is rejected instead (issue #085).
    ArithmeticOverflow = 5,
}

/// Storage keys used by the TWAP oracle contract.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    TokenA,
    TokenB,
    CumulativePrice,
    TotalTime,
    LastUpdateTimestamp,
    LastPrice,
    MinUpdateIntervalSeconds,
}

pub trait PriceOracle {
    fn latest_price(e: Env) -> i128;
}

#[contract]
/// TWAP Price Oracle for token pairs.
pub struct TwapOracle;

#[contractimpl]
impl TwapOracle {
    /// Initializes the TWAP oracle with token pair addresses and minimum update interval.
    ///
    /// # Parameters
    /// - `e`: Soroban environment.
    /// - `token_a`: Contract address of token A.
    /// - `token_b`: Contract address of token B.
    /// - `min_update_interval_seconds`: Minimum time in seconds between updates.
    ///
    /// # Returns
    /// - `Ok(())` when initialization succeeds.
    /// - `Err(Error::AlreadyInitialized)` if already initialized.
    pub fn initialize(
        e: Env,
        token_a: Address,
        token_b: Address,
        min_update_interval_seconds: u64,
    ) -> Result<(), Error> {
        if e.storage().instance().has(&DataKey::TokenA) {
            return Err(Error::AlreadyInitialized);
        }
        e.storage().instance().set(&DataKey::TokenA, &token_a);
        e.storage().instance().set(&DataKey::TokenB, &token_b);
        e.storage()
            .instance()
            .set(&DataKey::CumulativePrice, &0u128);
        e.storage().instance().set(&DataKey::TotalTime, &0u128);
        e.storage()
            .instance()
            .set(&DataKey::LastUpdateTimestamp, &0u64);
        e.storage().instance().set(&DataKey::LastPrice, &0u128);
        e.storage().instance().set(
            &DataKey::MinUpdateIntervalSeconds,
            &min_update_interval_seconds,
        );
        Ok(())
    }

    /// Updates the TWAP accumulator with a new price.
    /// Only allows updates after the minimum interval has elapsed.
    ///
    /// Every step of the accumulation — `price * elapsed` and both accumulator
    /// additions — is checked, and an update that would exceed the `u128` range
    /// is rejected with [`Error::ArithmeticOverflow`] rather than wrapping. A
    /// wrapped accumulator would silently poison every later TWAP, so failing
    /// loudly is the safe behaviour for a long-running feed (issue #085).
    ///
    /// # Parameters
    /// - `e`: Soroban environment.
    /// - `current_price`: The current price of token_b in terms of token_a (scaled appropriately).
    ///
    /// # Returns
    /// - `Ok(())` on success.
    /// - `Err(Error::NotInitialized)` if not initialized.
    /// - `Err(Error::InvalidPrice)` if price <= 0.
    /// - `Err(Error::InsufficientTimeElapsed)` if not enough time has passed since last update.
    /// - `Err(Error::ArithmeticOverflow)` if the accumulators would overflow.
    pub fn update_price(e: Env, current_price: i128) -> Result<(), Error> {
        if !e.storage().instance().has(&DataKey::TokenA) {
            return Err(Error::NotInitialized);
        }
        if current_price <= 0 {
            return Err(Error::InvalidPrice);
        }

        let now = e.ledger().timestamp();
        let last_update: u64 = e
            .storage()
            .instance()
            .get(&DataKey::LastUpdateTimestamp)
            .unwrap_or(0);
        let min_interval: u64 = e
            .storage()
            .instance()
            .get(&DataKey::MinUpdateIntervalSeconds)
            .unwrap_or(0);

        // The very first observation has no preceding price history, so it
        // contributes no elapsed time. `saturating_sub` keeps a rewound test
        // ledger from underflowing.
        let elapsed = if last_update == 0 {
            0
        } else {
            now.saturating_sub(last_update)
        };
        if last_update > 0 && elapsed < min_interval {
            return Err(Error::InsufficientTimeElapsed);
        }

        let last_price: u128 = e.storage().instance().get(&DataKey::LastPrice).unwrap_or(0);
        let cumulative: u128 = e
            .storage()
            .instance()
            .get(&DataKey::CumulativePrice)
            .unwrap_or(0);
        let total_time: u128 = e.storage().instance().get(&DataKey::TotalTime).unwrap_or(0);

        // Checked accumulation: a product or sum that does not fit in u128 aborts
        // the update instead of wrapping around into a nonsense TWAP.
        let weighted = last_price
            .checked_mul(elapsed as u128)
            .ok_or(Error::ArithmeticOverflow)?;
        let new_cumulative = cumulative
            .checked_add(weighted)
            .ok_or(Error::ArithmeticOverflow)?;
        let new_total_time = total_time
            .checked_add(elapsed as u128)
            .ok_or(Error::ArithmeticOverflow)?;

        e.storage()
            .instance()
            .set(&DataKey::CumulativePrice, &new_cumulative);
        e.storage()
            .instance()
            .set(&DataKey::TotalTime, &new_total_time);
        e.storage()
            .instance()
            .set(&DataKey::LastUpdateTimestamp, &now);
        e.storage()
            .instance()
            .set(&DataKey::LastPrice, &(current_price as u128));

        Ok(())
    }

    /// Returns the Time-Weighted Average Price since initialization.
    ///
    /// # Parameters
    /// - `e`: Soroban environment.
    ///
    /// # Returns
    /// - The TWAP as i128, or 0 if no updates.
    ///
    /// # Safety
    /// - The `as i128` cast on the division result assumes the TWAP fits within i128.
    ///   This is safe for all practical token prices given u128 range for the accumulator.
    pub fn get_twap(e: Env) -> i128 {
        let cumulative: u128 = e
            .storage()
            .instance()
            .get(&DataKey::CumulativePrice)
            .unwrap_or(0);
        let total_time: u128 = e.storage().instance().get(&DataKey::TotalTime).unwrap_or(0);
        if total_time == 0 {
            0
        } else {
            (cumulative / total_time) as i128
        }
    }

    /// Returns the token pair addresses.
    pub fn get_tokens(e: Env) -> (Address, Address) {
        let token_a: Address = e.storage().instance().get(&DataKey::TokenA).unwrap();
        let token_b: Address = e.storage().instance().get(&DataKey::TokenB).unwrap();
        (token_a, token_b)
    }
}

#[contractimpl]
impl PriceOracle for TwapOracle {
    /// Returns the latest TWAP price.
    fn latest_price(e: Env) -> i128 {
        Self::get_twap(e)
    }
}
