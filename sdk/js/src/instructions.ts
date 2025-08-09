import { address, getAddressDecoder, type Address } from "gill";

export const OP = {
  INIT: 0,
  DEPOSIT: 1,
  WITHDRAW: 2,
  DONATE: 3,
  POSTROOT: 4,
  CLAIM: 5,
} as const;

export function dataInit(decimals: number) {
  return Buffer.from([OP.INIT, decimals & 0xff]);
}

export function dataDeposit(amount: bigint, usdcDecimals: number) {
  const b = Buffer.alloc(1 + 8 + 1);
  b[0] = OP.DEPOSIT;
  b.writeBigUInt64LE(amount, 1);
  b[9] = usdcDecimals & 0xff;
  return b;
}

export function dataWithdraw(shares: bigint, usdcDecimals: number) {
  const b = Buffer.alloc(1 + 8 + 1);
  b[0] = OP.WITHDRAW;
  b.writeBigUInt64LE(shares, 1);
  b[9] = usdcDecimals & 0xff;
  return b;
}

export function dataDonate(amount: bigint, epoch: bigint, boostBps: number, usdcDecimals: number) {
  const b = Buffer.alloc(1 + 8 + 8 + 2 + 1);
  b[0] = OP.DONATE;
  b.writeBigUInt64LE(amount, 1);
  b.writeBigUInt64LE(epoch, 9);
  b.writeUInt16LE(boostBps, 17);
  b[19] = usdcDecimals & 0xff;
  return b;
}

export function dataPostRoot(epoch: bigint, totalWeight: bigint, root: Buffer) {
  const b = Buffer.alloc(1 + 8 + 16 + 32);
  b[0] = OP.POSTROOT;
  b.writeBigUInt64LE(epoch, 1);
  writeU128LE(totalWeight, b, 9);
  root.copy(b, 25);
  return b;
}

export function dataClaim(epoch: bigint, index: number, weight: bigint, proof: Buffer[]) {
  const b = Buffer.alloc(1 + 8 + 4 + 16 + 1 + 32 * proof.length);
  b[0] = OP.CLAIM;
  b.writeBigUInt64LE(epoch, 1);
  b.writeUInt32LE(index >>> 0, 9);
  writeU128LE(weight, b, 13);
  b[29] = proof.length & 0xff;
  proof.forEach((p, i) => p.copy(b, 30 + i * 32));
  return b;
}

function writeU128LE(n: bigint, out: Buffer, off: number) {
  let x = n;
  for (let i = 0; i < 16; i++) {
    out[off + i] = Number(x & 0xffn);
    x >>= 8n;
  }
}


