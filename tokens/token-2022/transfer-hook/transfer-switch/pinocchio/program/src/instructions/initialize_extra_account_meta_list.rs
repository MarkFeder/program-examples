use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::{rent::Rent, Sysvar},
    AccountView, Address, ProgramResult,
};
use pinocchio_log::log;
use pinocchio_system::instructions::CreateAccount;

use crate::instructions::EXTRA_ACCOUNT_METAS_SEED;

/// A serialized `ExtraAccountMetaList` holding this example's one extra
/// account: the sender's transfer switch.
///
/// The account is one TLV entry keyed by the `Execute` discriminator, so
/// Token-2022 can find the account list belonging to the instruction it is
/// about to CPI:
///
/// ```text
///   [105, 37, 101, 197, 75, 251, 102, 26]  Execute discriminator
///   [39, 0, 0, 0]                          value length (u32) = 4 + 1 * 35
///   [1, 0, 0, 0]                           account count (u32) = 1
///   ---- one 35-byte ExtraAccountMeta ----
///   [1]                                    address is a PDA of this program
///   [3, 3, 0 * 30]                         seed config, padded to 32 bytes
///   [0]                                    is_signer   = false
///   [0]                                    is_writable = false
/// ```
///
/// The seed config is `spl-tlv-account-resolution`'s packed form of
/// `Seed::AccountKey { index: 3 }`: a `3` tag, then the index. Account 3 of the
/// `Execute` call is the transfer authority, so Token-2022 derives
/// `[wallet]` against this program and passes the switch in — which is why no
/// caller ever has to name it.
///
/// The list is fixed for this example, so it is a constant rather than a
/// dependency on the TLV encoder.
#[rustfmt::skip]
const EXTRA_ACCOUNT_METAS_DATA: [u8; 51] = [
    105, 37, 101, 197, 75, 251, 102, 26,
    39, 0, 0, 0,
    1, 0, 0, 0,
    1,
    3, 3,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0,
    0,
];

/// Creates the `ExtraAccountMetaList` PDA for `mint`.
///
/// Token-2022 reads this account before every transfer to learn which accounts
/// beyond the four transfer accounts the hook expects. It must exist, otherwise
/// transfers of the mint fail.
///
/// Accounts:
///   0. `[signer, writable]` payer (funds the account)
///   1. `[writable]`         extra account meta list (PDA `[b"extra-account-metas", mint]`)
///   2. `[]`                 mint
///   3. `[]`                 system program
///
/// Instruction data: none beyond the interface discriminator.
pub fn initialize_extra_account_meta_list(program_id: &Address, accounts: &mut [AccountView]) -> ProgramResult {
    let [payer, extra_account_meta_list, mint, _system_program] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !payer.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    let (expected_address, bump) =
        Address::find_program_address(&[EXTRA_ACCOUNT_METAS_SEED, mint.address().as_ref()], program_id);
    if extra_account_meta_list.address() != &expected_address {
        return Err(ProgramError::InvalidSeeds);
    }

    let bump_bytes = [bump];
    let seeds = [Seed::from(EXTRA_ACCOUNT_METAS_SEED), Seed::from(mint.address().as_ref()), Seed::from(&bump_bytes)];

    let lamports = Rent::get()?.try_minimum_balance(EXTRA_ACCOUNT_METAS_DATA.len())?;

    log!("Creating extra account meta list");
    CreateAccount {
        from: payer,
        to: extra_account_meta_list,
        lamports,
        space: EXTRA_ACCOUNT_METAS_DATA.len() as u64,
        owner: program_id,
    }
    .invoke_signed(&[Signer::from(&seeds)])?;

    extra_account_meta_list.try_borrow_mut()?.copy_from_slice(&EXTRA_ACCOUNT_METAS_DATA);

    log!("Extra account meta list created");
    Ok(())
}
