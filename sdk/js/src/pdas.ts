import { address, getProgramDerivedAddress, getAddressEncoder, type Address } from "gill";

export const SEED_VAULT = Buffer.from("vault");
export const SEED_AUTH  = Buffer.from("vault_auth");
export const SEED_BOOST = Buffer.from("boost");
export const SEED_CLAIMS = Buffer.from("claims");

export async function deriveVaultPda(program: Address, usdcMint: Address, admin: Address) {
  const enc = getAddressEncoder();
  return getProgramDerivedAddress({
    programAddress: program,
    seeds: [SEED_VAULT, enc.encode(usdcMint), enc.encode(admin)]
  });
}

export async function deriveAuthPda(program: Address, vaultPda: Address) {
  const enc = getAddressEncoder();
  return getProgramDerivedAddress({
    programAddress: program,
    seeds: [SEED_AUTH, enc.encode(vaultPda)]
  });
}

export async function deriveBoostDistributor(program: Address, vaultPda: Address, epoch: bigint) {
  const enc = getAddressEncoder();
  const epochBuf = Buffer.alloc(8);
  epochBuf.writeBigUInt64LE(epoch);
  return getProgramDerivedAddress({
    programAddress: program,
    seeds: [SEED_BOOST, enc.encode(vaultPda), epochBuf]
  });
}

export async function deriveClaimsBitmap(program: Address, vaultPda: Address, epoch: bigint) {
  const enc = getAddressEncoder();
  const epochBuf = Buffer.alloc(8);
  epochBuf.writeBigUInt64LE(epoch);
  return getProgramDerivedAddress({
    programAddress: program,
    seeds: [SEED_CLAIMS, enc.encode(vaultPda), epochBuf]
  });
}


