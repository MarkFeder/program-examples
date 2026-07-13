use alloc::vec::Vec;

use pinocchio::{
    cpi::invoke,
    error::ProgramError,
    instruction::{InstructionAccount, InstructionView},
    sysvars::{rent::Rent, Sysvar},
    AccountView, ProgramResult,
};
use pinocchio_log::log;
use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::{instructions::InitializeMint2, state::Mint};

use crate::instructions::util::{push_borsh_string, read_borsh_string};

/// The Metaplex Token Metadata program ID
/// (`metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s`).
const TOKEN_METADATA_PROGRAM_ID: pinocchio::Address =
    pinocchio::Address::from_str_const("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");

/// Discriminator of the Metaplex `CreateMetadataAccountV3` instruction (variant
/// 33 of the Token Metadata program's instruction enum).
const CREATE_METADATA_ACCOUNT_V3: u8 = 33;

/// Borsh-encoded arguments for the create-token instruction.
///
/// Field order matches the `native` example's `CreateTokenArgs` so the two
/// options share an identical wire format.
pub struct CreateTokenArgs<'a> {
    pub name: &'a [u8],
    pub symbol: &'a [u8],
    pub uri: &'a [u8],
    pub decimals: u8,
}

impl<'a> CreateTokenArgs<'a> {
    /// Parses the instruction data: three Borsh strings followed by a `u8`.
    pub fn parse(data: &'a [u8]) -> Result<Self, ProgramError> {
        let mut offset = 0;
        let name = read_borsh_string(data, &mut offset)?;
        let symbol = read_borsh_string(data, &mut offset)?;
        let uri = read_borsh_string(data, &mut offset)?;
        let decimals = *data
            .get(offset)
            .ok_or(ProgramError::InvalidInstructionData)?;
        Ok(Self {
            name,
            symbol,
            uri,
            decimals,
        })
    }
}

/// Creates a new SPL Token mint and attaches an on-chain Metaplex metadata
/// account to it (name, symbol, URI).
///
/// Accounts:
///   0. `[signer, writable]` mint account (a fresh keypair to initialize)
///   1. `[]`                 mint authority (also recorded as metadata update authority)
///   2. `[writable]`         metadata account (the Metaplex metadata PDA)
///   3. `[signer, writable]` payer (funds the new accounts)
///   4. `[]`                 system program
///   5. `[]`                 token program
///   6. `[]`                 token metadata program
///
/// Instruction data: Borsh `[name: string, symbol: string, uri: string, decimals: u8]`.
///
/// The mint authority is passed as a non-signer; the metadata CPI requires it to
/// sign, which is satisfied by passing the payer's address for it (the payer
/// signs the transaction). This mirrors the `native` example.
pub fn create_token(accounts: &[AccountView], data: &[u8]) -> ProgramResult {
    // `token_program` and `token_metadata_program` are unused directly, but must
    // be supplied so they are present in the transaction for the CPIs below.
    let [mint_account, mint_authority, metadata_account, payer, system_program, _token_program, _token_metadata_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    let args = CreateTokenArgs::parse(data)?;

    // Fund the mint account with enough lamports to stay rent-exempt.
    let lamports = Rent::get()?.try_minimum_balance(Mint::LEN)?;

    log!("Creating mint account");
    CreateAccount {
        from: payer,
        to: mint_account,
        lamports,
        space: Mint::LEN as u64,
        owner: &pinocchio_token::ID,
    }
    .invoke()?;

    log!("Initializing mint account");
    InitializeMint2 {
        mint: mint_account,
        decimals: args.decimals,
        mint_authority: mint_authority.address(),
        freeze_authority: Some(mint_authority.address()),
    }
    .invoke()?;

    log!("Creating metadata account");
    let metadata_data = build_metadata_data(&args);
    let metadata_accounts = [
        InstructionAccount::writable(metadata_account.address()),
        InstructionAccount::readonly(mint_account.address()),
        InstructionAccount::readonly_signer(mint_authority.address()),
        InstructionAccount::writable_signer(payer.address()),
        // Update authority — recorded only, not required to sign for V3.
        InstructionAccount::readonly(mint_authority.address()),
        InstructionAccount::readonly(system_program.address()),
    ];
    let instruction = InstructionView {
        program_id: &TOKEN_METADATA_PROGRAM_ID,
        accounts: &metadata_accounts,
        data: &metadata_data,
    };
    invoke(
        &instruction,
        &[
            metadata_account,
            mint_account,
            mint_authority,
            payer,
            mint_authority,
            system_program,
        ],
    )?;

    log!("Token mint created successfully");
    Ok(())
}

/// Serializes the data for a Metaplex `CreateMetadataAccountV3` instruction.
///
/// Layout: `[33] DataV2 is_mutable:bool collection_details:Option`, where
/// `DataV2` is `name:string symbol:string uri:string seller_fee:u16
/// creators:Option collection:Option uses:Option`. Mirrors the values used by
/// the `anchor` and `native` examples (no royalties, no creators, immutable).
fn build_metadata_data(args: &CreateTokenArgs) -> Vec<u8> {
    let mut data = Vec::new();
    data.push(CREATE_METADATA_ACCOUNT_V3);

    // DataV2
    push_borsh_string(&mut data, args.name);
    push_borsh_string(&mut data, args.symbol);
    push_borsh_string(&mut data, args.uri);
    data.extend_from_slice(&0u16.to_le_bytes()); // seller_fee_basis_points
    data.push(0); // creators: None
    data.push(0); // collection: None
    data.push(0); // uses: None

    data.push(0); // is_mutable: false
    data.push(0); // collection_details: None

    data
}
