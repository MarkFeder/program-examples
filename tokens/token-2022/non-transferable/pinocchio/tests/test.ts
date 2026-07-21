import { Buffer } from "node:buffer";
import * as path from "node:path";
import {
  AccountRole,
  address,
  appendTransactionMessageInstruction,
  createTransactionMessage,
  generateKeyPairSigner,
  lamports,
  pipe,
  setTransactionMessageFeePayerSigner,
  signTransactionMessageWithSigners,
} from "@solana/kit";
import * as borsh from "borsh";
import { assert } from "chai";
import { FailedTransactionMetadata, LiteSVM } from "litesvm";

// LiteSVM's standard runtime bundles the SPL programs, so Token-2022 is already
// loaded — its ID is hard-coded here to avoid pulling in @solana/spl-token.
const TOKEN_2022_PROGRAM_ID = address("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
const SYSTEM_PROGRAM_ID = address("11111111111111111111111111111111");
const RENT_SYSVAR_ID = address("SysvarRent111111111111111111111111111111111");

// Borsh schema for the instruction data, matching the program's
// `CreateTokenArgs` (and the native example's wire format).
const CreateTokenArgsSchema: borsh.Schema = {
  struct: { token_decimals: "u8" },
};

// Token-2022 lays a mint with one (valueless) extension out as:
//   base account length (165) + account-type byte (1) + TLV entry (4) = 170
const EXTENDED_MINT_SIZE = 170;
const ACCOUNT_TYPE_OFFSET = 165; // 1 == Mint
const TLV_TYPE_OFFSET = 166; // u16 LE, 9 == NonTransferable
const TLV_LENGTH_OFFSET = 168; // u16 LE, 0 == the extension stores no value
const DECIMALS_OFFSET = 44; // in the base mint layout
const NON_TRANSFERABLE_EXTENSION = 9;
const ACCOUNT_TYPE_MINT = 1;

// The compiled program artifact, produced by `build-and-test` into ./fixtures.
// The npm scripts always run from the package root, so resolve from the cwd.
const PROGRAM_SO = path.join(process.cwd(), "tests", "fixtures", "token_2022_non_transferable_pinocchio_program.so");

describe("Token-2022 Non-Transferable (Pinocchio)", () => {
  let svm: LiteSVM;
  let programId: ReturnType<typeof address>;

  before(async () => {
    svm = new LiteSVM();
    // The program never asserts its own id, so any address works; a generated
    // one keeps the test self-contained.
    programId = (await generateKeyPairSigner()).address;
    svm.addProgramFromFile(programId, PROGRAM_SO);
  });

  it("Creates a Token-2022 non-transferable mint", async () => {
    const decimals = 9;
    const payer = await generateKeyPairSigner();
    svm.airdrop(payer.address, lamports(1_000_000_000n));

    const mint = await generateKeyPairSigner();

    const data = Buffer.from(borsh.serialize(CreateTokenArgsSchema, { token_decimals: decimals }));

    const ix = {
      programAddress: programId,
      accounts: [
        { address: mint.address, role: AccountRole.WRITABLE_SIGNER, signer: mint }, // mint account
        { address: payer.address, role: AccountRole.READONLY }, // mint authority
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
      (m) => svm.setTransactionMessageLifetimeUsingLatestBlockhash(m),
      (m) => appendTransactionMessageInstruction(ix, m),
    );

    const signedTx = await signTransactionMessageWithSigners(transactionMessage);
    const result = svm.sendTransaction(signedTx);
    if (result instanceof FailedTransactionMetadata) {
      throw new Error(`Transaction failed: ${result.err()}`);
    }

    const mintAccount = svm.getAccount(mint.address);
    if (!mintAccount?.exists) throw new Error("Mint account not found");
    const mintData = Buffer.from(mintAccount.data);

    // Owned by Token-2022, and sized for exactly one valueless extension.
    assert.equal(mintAccount.programAddress, TOKEN_2022_PROGRAM_ID);
    assert.equal(mintData.length, EXTENDED_MINT_SIZE);

    // Base mint fields were initialized.
    assert.equal(mintData[DECIMALS_OFFSET], decimals);

    // The extension header marks this as a Mint carrying NonTransferable, whose
    // TLV value is empty (length 0).
    assert.equal(mintData[ACCOUNT_TYPE_OFFSET], ACCOUNT_TYPE_MINT);
    assert.equal(mintData.readUInt16LE(TLV_TYPE_OFFSET), NON_TRANSFERABLE_EXTENSION);
    assert.equal(mintData.readUInt16LE(TLV_LENGTH_OFFSET), 0);

    console.log("Mint address:", mint.address);
  });
});
