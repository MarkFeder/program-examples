import { Buffer } from "node:buffer";
import * as path from "node:path";
import {
  AccountRole,
  address,
  appendTransactionMessageInstruction,
  createTransactionMessage,
  generateKeyPairSigner,
  getAddressEncoder,
  getProgramDerivedAddress,
  lamports,
  pipe,
  setTransactionMessageFeePayerSigner,
  signTransactionMessageWithSigners,
} from "@solana/kit";
import { SYSTEM_PROGRAM_ADDRESS } from "@solana-program/system";
import { ASSOCIATED_TOKEN_PROGRAM_ADDRESS, getTokenDecoder, TOKEN_PROGRAM_ADDRESS } from "@solana-program/token";
import * as borsh from "borsh";
import { assert } from "chai";
import { FailedTransactionMetadata, LiteSVM } from "litesvm";

// The legacy SPL Token and Associated Token Account programs are bundled with
// LiteSVM's standard runtime, and their ids come from the official
// @solana-program/token client. The Metaplex Token Metadata program is not
// bundled and has no official @solana-program client, so its id stays
// hand-rolled and it is dumped from mainnet into tests/fixtures by prepare.mjs.
const TOKEN_METADATA_PROGRAM_ID = address("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");

// Instruction discriminators (the Borsh enum variant index).
const CREATE = 0;
const MINT = 1;

// Borsh schema for the Create instruction data, matching the program's
// `CreateTokenArgs` (and the native example's wire format).
const CreateTokenArgsSchema: borsh.Schema = {
  struct: {
    instruction: "u8",
    nft_title: "string",
    nft_symbol: "string",
    nft_uri: "string",
  },
};

// The compiled program artifacts live in ./fixtures: the pinocchio program is
// built there by `build-and-test`, and token_metadata.so is dumped from mainnet
// by prepare.mjs. The npm scripts always run from the package root.
const FIXTURES = path.join(process.cwd(), "tests", "fixtures");
const PROGRAM_SO = path.join(FIXTURES, "nft_minter_pinocchio_program.so");
const TOKEN_METADATA_SO = path.join(FIXTURES, "token_metadata.so");

const addressEncoder = getAddressEncoder();

async function getMetadataAddress(mint: ReturnType<typeof address>) {
  const [metadata] = await getProgramDerivedAddress({
    programAddress: TOKEN_METADATA_PROGRAM_ID,
    seeds: ["metadata", addressEncoder.encode(TOKEN_METADATA_PROGRAM_ID), addressEncoder.encode(mint)],
  });
  return metadata;
}

async function getMasterEditionAddress(mint: ReturnType<typeof address>) {
  const [edition] = await getProgramDerivedAddress({
    programAddress: TOKEN_METADATA_PROGRAM_ID,
    seeds: ["metadata", addressEncoder.encode(TOKEN_METADATA_PROGRAM_ID), addressEncoder.encode(mint), "edition"],
  });
  return edition;
}

async function getAssociatedTokenAddress(mint: ReturnType<typeof address>, owner: ReturnType<typeof address>) {
  const [ata] = await getProgramDerivedAddress({
    programAddress: ASSOCIATED_TOKEN_PROGRAM_ADDRESS,
    seeds: [addressEncoder.encode(owner), addressEncoder.encode(TOKEN_PROGRAM_ADDRESS), addressEncoder.encode(mint)],
  });
  return ata;
}

describe("NFT Minter (Pinocchio)", () => {
  let svm: LiteSVM;
  let programId: ReturnType<typeof address>;
  let payer: Awaited<ReturnType<typeof generateKeyPairSigner>>;
  let mint: Awaited<ReturnType<typeof generateKeyPairSigner>>;

  before(async () => {
    svm = new LiteSVM();
    // The program never asserts its own id, so any address works; a generated
    // one keeps the test self-contained.
    programId = (await generateKeyPairSigner()).address;
    svm.addProgramFromFile(programId, PROGRAM_SO);
    svm.addProgramFromFile(TOKEN_METADATA_PROGRAM_ID, TOKEN_METADATA_SO);

    payer = await generateKeyPairSigner();
    svm.airdrop(payer.address, lamports(10_000_000_000n));
    // The mint is created in the first test and reused (as a non-signer) by the
    // second, so it is generated once for the whole suite.
    mint = await generateKeyPairSigner();
  });

  async function send<TInstruction extends Parameters<typeof appendTransactionMessageInstruction>[0]>(
    ix: TInstruction,
  ) {
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
  }

  it("Create an NFT!", async () => {
    const metadataAddress = await getMetadataAddress(mint.address);

    const data = Buffer.from(
      borsh.serialize(CreateTokenArgsSchema, {
        instruction: CREATE,
        nft_title: "Homer NFT",
        nft_symbol: "HOMR",
        nft_uri:
          "https://raw.githubusercontent.com/solana-developers/program-examples/new-examples/tokens/tokens/.assets/nft.json",
      }),
    );

    await send({
      programAddress: programId,
      accounts: [
        { address: mint.address, role: AccountRole.WRITABLE_SIGNER, signer: mint }, // mint account
        { address: payer.address, role: AccountRole.READONLY }, // mint authority
        { address: metadataAddress, role: AccountRole.WRITABLE }, // metadata account
        { address: payer.address, role: AccountRole.WRITABLE_SIGNER, signer: payer }, // payer
        { address: SYSTEM_PROGRAM_ADDRESS, role: AccountRole.READONLY }, // system program
        { address: TOKEN_PROGRAM_ADDRESS, role: AccountRole.READONLY }, // token program
        { address: TOKEN_METADATA_PROGRAM_ID, role: AccountRole.READONLY }, // token metadata program
      ],
      data: new Uint8Array(data),
    });

    const mintAccount = svm.getAccount(mint.address);
    if (!mintAccount?.exists) throw new Error("Mint account not found");
    assert.equal(mintAccount.programAddress, TOKEN_PROGRAM_ADDRESS);

    const metadataAccount = svm.getAccount(metadataAddress);
    if (!metadataAccount?.exists) throw new Error("Metadata account not found");
    assert.equal(metadataAccount.programAddress, TOKEN_METADATA_PROGRAM_ID);
    assert.isTrue(Buffer.from(metadataAccount.data).toString("utf-8").includes("Homer NFT"));
  });

  it("Mint the NFT to your wallet!", async () => {
    const metadataAddress = await getMetadataAddress(mint.address);
    const editionAddress = await getMasterEditionAddress(mint.address);
    const ata = await getAssociatedTokenAddress(mint.address, payer.address);

    await send({
      programAddress: programId,
      accounts: [
        { address: mint.address, role: AccountRole.WRITABLE }, // mint account
        { address: metadataAddress, role: AccountRole.WRITABLE }, // metadata account
        { address: editionAddress, role: AccountRole.WRITABLE }, // master edition account
        { address: payer.address, role: AccountRole.READONLY }, // mint authority
        { address: ata, role: AccountRole.WRITABLE }, // associated token account
        { address: payer.address, role: AccountRole.WRITABLE_SIGNER, signer: payer }, // payer
        { address: SYSTEM_PROGRAM_ADDRESS, role: AccountRole.READONLY }, // system program
        { address: TOKEN_PROGRAM_ADDRESS, role: AccountRole.READONLY }, // token program
        { address: ASSOCIATED_TOKEN_PROGRAM_ADDRESS, role: AccountRole.READONLY }, // associated token program
        { address: TOKEN_METADATA_PROGRAM_ID, role: AccountRole.READONLY }, // token metadata program
      ],
      data: new Uint8Array([MINT]),
    });

    // The NFT (a single token) landed in the payer's associated token account.
    const ataAccount = svm.getAccount(ata);
    if (!ataAccount?.exists) throw new Error("Associated token account not found");
    // Decode the token account with the official codec instead of reading the
    // `amount` field from a raw byte offset by hand.
    const amount = getTokenDecoder().decode(ataAccount.data).amount;
    assert.equal(amount, 1n);

    // The master edition account exists and is owned by the Token Metadata
    // program — proof the CreateMasterEditionV3 CPI succeeded.
    const editionAccount = svm.getAccount(editionAddress);
    if (!editionAccount?.exists) throw new Error("Master edition account not found");
    assert.equal(editionAccount.programAddress, TOKEN_METADATA_PROGRAM_ID);
  });
});
