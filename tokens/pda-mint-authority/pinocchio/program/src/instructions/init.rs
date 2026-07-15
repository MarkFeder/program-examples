use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::rent::{ACCOUNT_STORAGE_OVERHEAD, DEFAULT_LAMPORTS_PER_BYTE},
    AccountView, Address, ProgramResult,
};
use pinocchio_log::log;
use pinocchio_pubkey::derive_address;
use pinocchio_system::instructions::CreateAccount;

use crate::state::MintAuthorityPda;

/// Creates the mint-authority PDA account and stores its canonical bump.
///
/// The PDA (`[b"mint_authority"]`) becomes the mint and freeze authority for
/// every token minted by this program, so a single program-owned address — not
/// any wallet — controls minting.
///
/// Accounts:
///   0. `[writable]`         mint authority PDA (created here)
///   1. `[signer, writable]` payer (funds the new account)
///   2. `[]`                 system program
///
/// Instruction data: `[bump: u8]` (the canonical bump for the PDA).
pub fn init(program_id: &Address, accounts: &[AccountView], data: &[u8]) -> ProgramResult {
    let [mint_authority, payer, _system_program] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    let bump = *data.first().ok_or(ProgramError::InvalidInstructionData)?;

    // Verify the supplied account is the canonical PDA for this bump.
    let pda = derive_address(
        &[MintAuthorityPda::SEED_PREFIX],
        Some(bump),
        program_id.as_array(),
    );
    if mint_authority.address().as_array() != &pda {
        return Err(ProgramError::InvalidSeeds);
    }

    // Rent computed with integer math using the network's default parameters
    // (`DEFAULT_LAMPORTS_PER_BYTE` already folds in the 2-year exemption
    // threshold). We avoid `Rent::try_minimum_balance`, whose floating-point
    // exemption-threshold path emits an instruction the bankrun test VM rejects
    // ("unsupported BPF instruction").
    let lamports = (ACCOUNT_STORAGE_OVERHEAD + MintAuthorityPda::ACCOUNT_SPACE as u64)
        * DEFAULT_LAMPORTS_PER_BYTE;
    let bump_bytes = [bump];
    let seeds = [
        Seed::from(MintAuthorityPda::SEED_PREFIX),
        Seed::from(&bump_bytes),
    ];
    let signers = [Signer::from(&seeds)];

    log!("Creating mint authority PDA");
    CreateAccount {
        from: payer,
        to: mint_authority,
        lamports,
        space: MintAuthorityPda::ACCOUNT_SPACE as u64,
        owner: program_id,
    }
    .invoke_signed(&signers)?;

    MintAuthorityPda { bump }.serialize(&mut mint_authority.try_borrow_mut()?)?;

    log!("Mint authority PDA created successfully");
    Ok(())
}
