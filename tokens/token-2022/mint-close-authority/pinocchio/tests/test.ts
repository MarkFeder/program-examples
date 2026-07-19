import { Buffer } from "node:buffer";
import {
  AccountRole,
  address,
  appendTransactionMessageInstruction,
  blockhash,
  createKeyPairSignerFromBytes,
  createTransactionMessage,
  generateKeyPairSigner,
  getAddressEncoder,
  getTransactionEncoder,
  pipe,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  signTransactionMessageWithSigners,
} from "@solana/kit";
// solana-bankrun@0.3.1's BanksClient API is still expressed in @solana/web3.js
// types: `start()` and `getAccount()` take a `PublicKey`, and
// `processTransaction()` takes a `VersionedTransaction`. Those three are the
// only web3.js touch-points — a thin interop shim around bankrun. Everything
// the test *builds* (addresses, the instruction, the transaction message, and
// signing) uses @solana/kit.
import { PublicKey, VersionedTransaction } from "@solana/web3.js";
import * as borsh from "borsh";
import { assert } from "chai";
import { start } from "solana-bankrun";

// The Token-2022 program is bundled with bankrun, so there is no fixture to
// load. Its ID is hard-coded here to avoid pulling in @solana/spl-token.
const TOKEN_2022_PROGRAM_ID = address("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
const SYSTEM_PROGRAM_ID = address("11111111111111111111111111111111");
const RENT_SYSVAR_ID = address("SysvarRent111111111111111111111111111111111");

// Borsh schema for the instruction data, matching the program's
// `CreateTokenArgs` (and the native example's wire format).
const CreateTokenArgsSchema: borsh.Schema = {
  struct: { token_decimals: "u8" },
};

// Token-2022 lays a mint with one extension out as:
//   base account length (165) + account-type byte (1) + TLV entry (36) = 202
const EXTENDED_MINT_SIZE = 202;
const ACCOUNT_TYPE_OFFSET = 165; // 1 == Mint
const TLV_TYPE_OFFSET = 166; // u16 LE, 3 == MintCloseAuthority
const TLV_LENGTH_OFFSET = 168; // u16 LE, 32 == byte length of the value
const TLV_VALUE_OFFSET = 170; // 32-byte close authority pubkey
const DECIMALS_OFFSET = 44; // in the base mint layout
const MINT_CLOSE_AUTHORITY_EXTENSION = 3;
const ACCOUNT_TYPE_MINT = 1;

const addressEncoder = getAddressEncoder();

describe("Token-2022 Mint Close Authority (Pinocchio)", () => {
  // bankrun's start() wants a web3.js PublicKey for the program id; convert it
  // to a kit address for the instruction.
  const programPubkey = PublicKey.unique();
  const PROGRAM_ID = address(programPubkey.toBase58());

  let context: Awaited<ReturnType<typeof start>>;
  let client: (typeof context)["banksClient"];

  // A `describe` callback runs synchronously, so the async bankrun setup must
  // live in a `before` hook — otherwise the `it` blocks register after Mocha
  // has already collected the suite and nothing runs.
  before(async () => {
    context = await start(
      [{ name: "token_2022_mint_close_authority_pinocchio_program", programId: programPubkey }],
      [],
    );
    client = context.banksClient;
  });

  it("Creates a Token-2022 mint with a close authority", async () => {
    const decimals = 9;
    // bankrun funds a web3.js Keypair; re-key it as a kit signer so kit can sign.
    const payer = await createKeyPairSignerFromBytes(context.payer.secretKey);
    const mint = await generateKeyPairSigner();
    // A distinct key for the close authority so the stored-authority assertion
    // verifies it is sourced from account index 2, not the mint authority/payer.
    const closeAuthority = await generateKeyPairSigner();

    const data = Buffer.from(borsh.serialize(CreateTokenArgsSchema, { token_decimals: decimals }));

    const ix = {
      programAddress: PROGRAM_ID,
      accounts: [
        { address: mint.address, role: AccountRole.WRITABLE_SIGNER, signer: mint }, // mint account
        { address: payer.address, role: AccountRole.READONLY }, // mint authority
        { address: closeAuthority.address, role: AccountRole.READONLY }, // close authority
        { address: payer.address, role: AccountRole.WRITABLE_SIGNER, signer: payer }, // payer
        { address: RENT_SYSVAR_ID, role: AccountRole.READONLY }, // rent sysvar
        { address: SYSTEM_PROGRAM_ID, role: AccountRole.READONLY }, // system program
        { address: TOKEN_2022_PROGRAM_ID, role: AccountRole.READONLY }, // Token-2022 program
      ],
      data: new Uint8Array(data),
    };

    const transactionMessage = pipe(
      createTransactionMessage({ version: 0 }),
      (m) => setTransactionMessageFeePayerSigner(payer, m),
      (m) =>
        setTransactionMessageLifetimeUsingBlockhash(
          { blockhash: blockhash(context.lastBlockhash), lastValidBlockHeight: 0n },
          m,
        ),
      (m) => appendTransactionMessageInstruction(ix, m),
    );

    const signedTx = await signTransactionMessageWithSigners(transactionMessage);
    const wireBytes = new Uint8Array(getTransactionEncoder().encode(signedTx));
    await client.processTransaction(VersionedTransaction.deserialize(wireBytes));

    const mintAccount = await client.getAccount(new PublicKey(mint.address));
    if (mintAccount === null) throw new Error("Mint account not found");
    const mintData = Buffer.from(mintAccount.data);

    // Owned by Token-2022, and sized for exactly one extension.
    assert.deepEqual(
      new Uint8Array(mintAccount.owner.toBytes()),
      new Uint8Array(addressEncoder.encode(TOKEN_2022_PROGRAM_ID)),
    );
    assert.equal(mintData.length, EXTENDED_MINT_SIZE);

    // Base mint fields were initialized.
    assert.equal(mintData[DECIMALS_OFFSET], decimals);

    // The extension header marks this as a Mint carrying MintCloseAuthority.
    assert.equal(mintData[ACCOUNT_TYPE_OFFSET], ACCOUNT_TYPE_MINT);
    assert.equal(mintData.readUInt16LE(TLV_TYPE_OFFSET), MINT_CLOSE_AUTHORITY_EXTENSION);
    assert.equal(mintData.readUInt16LE(TLV_LENGTH_OFFSET), 32);

    // The configured close authority was stored in the extension.
    const storedCloseAuthority = mintData.subarray(TLV_VALUE_OFFSET, TLV_VALUE_OFFSET + 32);
    assert.deepEqual(
      new Uint8Array(storedCloseAuthority),
      new Uint8Array(addressEncoder.encode(closeAuthority.address)),
    );

    console.log("Mint address:", mint.address);
  });
});
