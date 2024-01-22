import * as anchor from "@coral-xyz/anchor";
import { Program, BN, Address } from "@coral-xyz/anchor";
import { FlashLoan, IDL } from "../target/types/flash_loan";
import {
  Keypair,
  PublicKey,
  LAMPORTS_PER_SOL,
  SYSVAR_INSTRUCTIONS_PUBKEY,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  createMint,
  createMintToInstruction,
  createTransferInstruction,
  getOrCreateAssociatedTokenAccount,
  mintTo,
} from "@solana/spl-token";

describe("flash-loan", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const provider = anchor.getProvider();
  const connection = provider.connection;
  const program = new Program<FlashLoan>(IDL, "4z55dAv3ySKCbDKUwG25cUbCyhNcqG7T5ze9aiYr4wpr" as Address, provider);


  const confirm = async (signature: string): Promise<string> => {
    const block = await connection.getLatestBlockhash();
    await connection.confirmTransaction({
      signature,
      ...block,
    });
    return signature;
  };

  const log = async (signature: string): Promise<string> => {
    console.log(
      `Your transaction signature: https://explorer.solana.com/transaction/${signature}?cluster=custom&customUrl=${connection.rpcEndpoint}`
    );
    return signature;
  };

  const borrower = Keypair.generate();

  let protocol: PublicKey;
  let mint: PublicKey;
  let borrowerAta: PublicKey;
  let protocolAta: PublicKey;

  it("Airdrop, Create Ata, Mint", async () => {
    protocol = PublicKey.findProgramAddressSync([Buffer.from("protocol")], program.programId)[0];

    await connection.requestAirdrop(borrower.publicKey, LAMPORTS_PER_SOL * 10).then(confirm).then(log);
    await connection.requestAirdrop(protocol, LAMPORTS_PER_SOL * 10).then(confirm).then(log);

    mint = await createMint(connection, borrower, borrower.publicKey, null, 6);
    borrowerAta = (await getOrCreateAssociatedTokenAccount(connection, borrower, mint, borrower.publicKey)).address;
    protocolAta = (await getOrCreateAssociatedTokenAccount(connection, borrower, mint, protocol, true)).address;
    
    await mintTo(connection, borrower, mint, protocolAta, borrower.publicKey, 1e7);
  
  });

  it("Create Ata, Mint", async () => {
  
  });

  it("Flash Loan", async () => {
    let borrowAmount = new BN(1e6);

    let borrowIx = await program.methods
    .borrow(borrowAmount)
    .accounts({
      borrower: borrower.publicKey,
      protocol,
      mint,
      borrowerAta,
      protocolAta,
      instructions: SYSVAR_INSTRUCTIONS_PUBKEY,
      tokenProgram: TOKEN_PROGRAM_ID,
      associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      systemProgram: SystemProgram.programId 
    })
    .instruction();

    let randomIx = createMintToInstruction(mint, borrowerAta, borrower.publicKey, 1e6);

    let repayIx = await program.methods
    .repay()
    .accounts({
      borrower: borrower.publicKey,
      protocol,
      mint,
      borrowerAta,
      protocolAta,
      instructions: SYSVAR_INSTRUCTIONS_PUBKEY,
      tokenProgram: TOKEN_PROGRAM_ID,
      associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      systemProgram: SystemProgram.programId 
    })
    .instruction();

    let tx = new Transaction().add(borrowIx).add(randomIx).add(repayIx);
    await provider.sendAndConfirm(tx, [ borrower ], {skipPreflight: true}).then(confirm).then(log);
  })
});
