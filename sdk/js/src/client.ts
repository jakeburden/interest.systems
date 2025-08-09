import {
  address, Address,
  createSolanaClient, createTransaction, signTransactionMessageWithSigners,
  getAssociatedTokenAccountAddress, TOKEN_PROGRAM_ADDRESS,
  getAddressEncoder,
} from "gill";
import { dataInit, dataDeposit, dataWithdraw, dataDonate, dataPostRoot, dataClaim } from "./instructions.js";
import { deriveVaultPda, deriveBoostDistributor, deriveClaimsBitmap } from "./pdas.js";

export type Accounts = {
  program: Address;
  admin: Address;
  operator: Address;
  usdcMint: Address;
  shareMint: Address;
  vaultUsdcAta: Address;
  boostUsdcAta: Address;
};

export function initClient(urlOrMoniker: string = "localnet") {
  const { rpc, rpcSubscriptions, sendAndConfirmTransaction } = createSolanaClient({ urlOrMoniker });
  return { rpc, rpcSubscriptions, sendAndConfirmTransaction };
}

export async function buildInitializeIx(acc: Accounts, decimals = 6) {
  const [vaultPda] = await deriveVaultPda(acc.program, acc.usdcMint, acc.admin);
  return {
    programId: acc.program,
    keys: [
      { pubkey: acc.program, isSigner: false, isWritable: false }, // kept for readability
    ],
    accounts: [
      // must be provided by caller in tx: vault_state(w), admin(s), operator, usdcMint, shareMint, vaultPda
    ],
    data: dataInit(decimals),
    vaultPda
  };
}

export async function buildDepositIx(acc: Accounts, user: Address, userUsdcAta: Address, userShareAta: Address, amount: bigint) {
  const [vaultPda] = await deriveVaultPda(acc.program, acc.usdcMint, acc.admin);
  return {
    data: dataDeposit(amount, 6),
    accounts: [
      // vault_state(w), vault_pda, user(s), user_usdc_ata(w), vault_usdc_ata(w),
      // share_mint(w), user_share_ata(w), token_program, usdc_mint
    ],
    vaultPda,
  };
}

// Similar helpers for withdraw/donate/postRoot/claim ...

// Convenience submitter
export async function sendIxs(urlOrMoniker: string, feePayer: any, ixs: any[]) {
  const { rpc, sendAndConfirmTransaction } = createSolanaClient({ urlOrMoniker });
  const { value: latestBlockhash } = await rpc.getLatestBlockhash().send();

  const tx = createTransaction({
    version: "legacy",
    feePayer,
    latestBlockhash,
    instructions: ixs as any
  });

  const signed = await signTransactionMessageWithSigners(tx);
  const sig = await sendAndConfirmTransaction(signed);
  return sig;
}


