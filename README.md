# interest.systems

Non-custodial, transparent, liquid yield from Solana validator rewards, paid in USDC.

- Non-custodial: Funds live in a program-derived account (PDA); users hold redeemable vault shares.
- Transparent: Price-per-share (PPS) and boost accounting are on-chain; Merkle roots are posted per epoch.
- Liquid: USDC stays in-vault; deposit and withdraw at any time.

## How it works
1) Deposit USDC → receive vault shares
   - Users deposit USDC to the vault and receive fungible vault shares. PPS starts at 1e12 (RAY) and increases as rewards are donated.
2) Validator rewards → USDC → donate
   - The operator swaps SOL rewards to USDC off-chain, then calls DonateReward(amount, epoch, boost_bps).
   - Base portion increases PPS for all share holders; boost portion is set aside for delegators of that validator.
3) Delegator boost
   - Operator posts a Merkle root of delegator weights via PostRoot(epoch, total_weight, root).
   - Delegators claim USDC boost with Claim(epoch, index, weight, proof).
4) Withdraw anytime
   - Burn vault shares and receive USDC equal to shares * PPS / RAY.

## Monorepo
```
interest.systems/
├─ programs/interest_vault    # Pinocchio on-chain program
├─ sdk/js                     # Gill TypeScript SDK (PDAs, ix data, helpers)
├─ tests/litesvm              # Fast Rust LiteSVM smoke tests
├─ surfpool                   # Runbooks for deploy/E2E
├─ scripts                    # Build/dev scripts
└─ site                       # Placeholder site
```

## On-chain program
- Pinocchio entrypoint + zero-copy parsing.
- SPL Token checked CPIs (TransferChecked, MintToChecked, BurnChecked).
- Merkle proofs via Solana keccak256 syscall.

### State
- VaultState: admin, operator, usdc_mint, share_mint, vault_pda, total_shares (u128), pps (u128, RAY=1e12), buffered_base.
- BoostDistributor (per epoch): epoch, root[32], total_weight (u128), boost_total (u64).
- ClaimBitmap256: 256-bit claim bitmap (MVP).

### PDAs (seeds)
- Vault: [b"vault", usdc_mint, admin]
- Boost: [b"boost", vault_pda, epoch_le]
- Claims bitmap: [b"claims", vault_pda, epoch_le]

### Instructions
- InitializeVault(decimals)
- Deposit(amount, usdc_decimals)
- Withdraw(shares, usdc_decimals)
- DonateReward(amount, epoch, boost_bps, usdc_decimals)
- PostRoot(epoch, total_weight, root)
- Claim(epoch, index, weight, proof[])

## SDK (Gill)
- PDA helpers via getProgramDerivedAddress.
- Instruction data builders for all ops.
- Transaction helpers using createSolanaClient and signTransactionMessageWithSigners.

## Build & test
- Build SBF program
  ```bash
  ./scripts/build-program.sh
  ```
- LiteSVM smoke tests (build .so first)
  ```bash
  cargo test -p interest_litesvm_tests
  ```
- SDK (Node)
  ```bash
  cd sdk/js
  npm i && npm run build
  ```
- Surfpool (local)
  ```bash
  surfpool start
  # UI: run "Deploy Local" (provide program .so/keypair), then "E2E Local"
  ```

## Trust & risks
- Non-custodial: USDC held by PDA; withdraw via PPS at any time.
- Operator: can donate rewards and post Merkle roots; cannot seize user funds.
- Risks: SOL→USDC swap execution; correctness of posted roots/weights; SPL Token/USDC mint assumptions.

## Roadmap
- IDL export for auto-encoding in Surfpool.
- Harvester CLI (swap, donate, post root).
- Sharded/extended claim bitmaps.

## License
Apache-2.0
