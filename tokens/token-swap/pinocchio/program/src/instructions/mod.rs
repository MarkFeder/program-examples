mod create_amm;
mod create_pool;
mod deposit_liquidity;
mod swap_exact_tokens_for_tokens;
mod withdraw_liquidity;

pub use create_amm::*;
pub use create_pool::*;
pub use deposit_liquidity::*;
pub use swap_exact_tokens_for_tokens::*;
pub use withdraw_liquidity::*;

use pinocchio::{cpi::Seed, AccountView, Address};

use crate::{
    error::SwapError,
    state::{read_pool, AUTHORITY_SEED},
};

/// The pool's PDAs, all seeded from the same three addresses.
pub struct PoolSeeds {
    pub amm: [u8; 32],
    pub mint_a: [u8; 32],
    pub mint_b: [u8; 32],
    pub authority_bump: u8,
}

impl PoolSeeds {
    /// Reads the pool account and rederives the authority, checking both the
    /// pool and the authority accounts the caller supplied.
    ///
    /// The pool records which mints it is for, so binding the mints to the pool
    /// here is what stops a caller pairing a real pool with someone else's
    /// token accounts.
    pub fn load(
        program_id: &Address,
        pool: &AccountView,
        pool_authority: &AccountView,
        mint_a: &AccountView,
        mint_b: &AccountView,
    ) -> Result<Self, pinocchio::error::ProgramError> {
        if !pool.owned_by(program_id) {
            return Err(SwapError::InvalidAccountData.into());
        }

        let stored = read_pool(&pool.try_borrow()?)?;

        if stored.mint_a != mint_a.address().as_ref() || stored.mint_b != mint_b.address().as_ref() {
            return Err(SwapError::InvalidMint.into());
        }
        let amm = stored.amm;

        let (pool_address, _) =
            Address::find_program_address(&[&amm, mint_a.address().as_ref(), mint_b.address().as_ref()], program_id);
        if pool.address() != &pool_address {
            return Err(SwapError::InvalidSeeds.into());
        }

        let (authority_address, authority_bump) = Address::find_program_address(
            &[&amm, mint_a.address().as_ref(), mint_b.address().as_ref(), AUTHORITY_SEED],
            program_id,
        );
        if pool_authority.address() != &authority_address {
            return Err(SwapError::InvalidSeeds.into());
        }

        Ok(Self { amm, mint_a: stored.mint_a, mint_b: stored.mint_b, authority_bump })
    }

    /// The signer seeds for the pool authority, which owns the pool's token
    /// accounts and the liquidity mint.
    pub fn authority_seeds<'a>(&'a self, bump: &'a [u8; 1]) -> [Seed<'a>; 5] {
        [
            Seed::from(&self.amm),
            Seed::from(&self.mint_a),
            Seed::from(&self.mint_b),
            Seed::from(AUTHORITY_SEED),
            Seed::from(bump),
        ]
    }
}

/// Confirms `account` is the PDA for `seeds`, returning its bump.
pub fn expect_pda(
    program_id: &Address,
    account: &AccountView,
    seeds: &[&[u8]],
) -> Result<u8, pinocchio::error::ProgramError> {
    let (address, bump) = Address::find_program_address(seeds, program_id);
    if account.address() != &address {
        return Err(SwapError::InvalidSeeds.into());
    }
    Ok(bump)
}

/// `a * b / c` in `u128`, so the product of two `u64` amounts cannot overflow.
pub fn mul_div(a: u64, b: u64, c: u64) -> Result<u64, pinocchio::error::ProgramError> {
    let result = (a as u128)
        .checked_mul(b as u128)
        .ok_or(SwapError::MathOverflow)?
        .checked_div(c as u128)
        .ok_or(SwapError::MathOverflow)?;
    u64::try_from(result).map_err(|_| SwapError::MathOverflow.into())
}
