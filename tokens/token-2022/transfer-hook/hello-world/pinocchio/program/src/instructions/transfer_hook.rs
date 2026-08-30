use pinocchio::{error::ProgramError, AccountView, Address, ProgramResult};
use pinocchio_log::log;

use crate::{
    error::TransferHookError,
    instructions::EXTRA_ACCOUNT_METAS_SEED,
    token2022::{get_extension_data, TRANSFER_HOOK_ACCOUNT},
};

/// The `Execute` instruction of the transfer-hook interface: Token-2022 CPIs
/// this during every transfer of a mint that names this program as its hook.
///
/// The account order is fixed by the interface — the four transfer accounts,
/// then the `ExtraAccountMetaList`, then whatever extra accounts that list
/// resolves to (none in this example).
///
/// Accounts:
///   0. `[]` source token account
///   1. `[]` mint
///   2. `[]` destination token account
///   3. `[]` source token account owner
///   4. `[]` extra account meta list (PDA `[b"extra-account-metas", mint]`)
///
/// Instruction data: `[amount: u64 (LE)]`, unused here.
pub fn transfer_hook(program_id: &Address, accounts: &mut [AccountView], _data: &[u8]) -> ProgramResult {
    let [source_token, mint, _destination_token, _owner, extra_account_meta_list, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Anyone can call this instruction directly, so confirm the account list
    // really belongs to this mint rather than trusting the caller's choice.
    let (expected_address, _) =
        Address::find_program_address(&[EXTRA_ACCOUNT_METAS_SEED, mint.address().as_ref()], program_id);
    if extra_account_meta_list.address() != &expected_address {
        return Err(ProgramError::InvalidSeeds);
    }

    check_is_transferring(source_token)?;

    log!("Hello Transfer Hook!");
    Ok(())
}

/// Fails unless the source account is mid-transfer.
///
/// Token-2022 raises the `TransferHookAccount` extension's `transferring` flag
/// only for the duration of the transfer it is executing. Checking it is what
/// stops the hook from being invoked directly, outside any transfer — a real
/// hook that grants or records something must not be callable on its own.
fn check_is_transferring(source_token: &AccountView) -> ProgramResult {
    let account_data = source_token.try_borrow()?;
    let extension = get_extension_data(&account_data, TRANSFER_HOOK_ACCOUNT)
        .ok_or(TransferHookError::IsNotCurrentlyTransferring)?;

    match extension.first() {
        Some(1) => Ok(()),
        _ => Err(TransferHookError::IsNotCurrentlyTransferring.into()),
    }
}
