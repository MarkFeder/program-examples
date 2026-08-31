mod chop_tree;
mod init_player;
mod mint_nft;

pub use chop_tree::*;
pub use init_player::*;
pub use mint_nft::*;

use pinocchio::{
    sysvars::{rent::Rent, Sysvar},
    AccountView, Address, ProgramResult,
};
use pinocchio_system::instructions::Transfer;

use crate::error::GameError;

/// Tops an account up to rent exemption for its current size.
///
/// Writing token metadata reallocates the mint, so it can drop below the
/// minimum after a longer value is stored.
pub fn top_up_rent(payer: &AccountView, account: &AccountView) -> ProgramResult {
    let required = Rent::get()?.try_minimum_balance(account.data_len())?;
    let current = account.lamports();
    if required > current {
        Transfer { from: payer, to: account, lamports: required - current }.invoke()?;
    }
    Ok(())
}

/// The SPL Token-2022 program ID
/// (`TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb`).
pub const TOKEN_2022_PROGRAM_ID: Address =
    pinocchio::Address::from_str_const("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

/// Confirms `account` is the PDA for `seeds`, returning its bump.
pub fn expect_pda(
    program_id: &Address,
    account: &AccountView,
    seeds: &[&[u8]],
) -> Result<u8, pinocchio::error::ProgramError> {
    let (address, bump) = Address::find_program_address(seeds, program_id);
    if account.address() != &address {
        return Err(GameError::InvalidSeeds.into());
    }
    Ok(bump)
}

/// Splits `[len: u8, bytes]` off the front of instruction data.
///
/// The level seed is caller-chosen, so it is length-prefixed rather than
/// running to the end — that keeps room for fields after it.
pub fn read_prefixed(data: &[u8], offset: usize) -> Result<&[u8], pinocchio::error::ProgramError> {
    let len = *data.get(offset).ok_or(pinocchio::error::ProgramError::InvalidInstructionData)? as usize;
    let start = offset + 1;
    data.get(start..start + len).ok_or(pinocchio::error::ProgramError::InvalidInstructionData)
}
