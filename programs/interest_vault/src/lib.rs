#![cfg_attr(not(feature = "std"), no_std)]

use bytemuck::{Pod, Zeroable};
use core::mem::size_of;
use pinocchio::{
    account_info::AccountInfo,
    cpi,
    entrypoint,
    instruction::{AccountMeta, Instruction, Signer},
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    syscalls,
    ProgramResult,
};

entrypoint!(process_instruction);

// ---------- Constants ----------
const RAY: u128 = 1_000_000_000_000; // 1e12 fixed point PPS
const SEED_VAULT: &[u8] = b"vault";
const SEED_AUTH: &[u8]  = b"vault_auth";
const SEED_BOOST: &[u8] = b"boost";
const SEED_CLAIMS: &[u8] = b"claims";

// SPL Token discriminants (spl_token::instruction::TokenInstruction)
const IX_TRANSFER_CHECKED: u8 = 12;
const IX_MINT_TO_CHECKED:  u8 = 14;
const IX_BURN_CHECKED:     u8 = 15;

// Our instruction tags
const OP_INIT:    u8 = 0;
const OP_DEPOSIT: u8 = 1;
const OP_WITHDRAW:u8 = 2;
const OP_DONATE:  u8 = 3;
const OP_POSTROOT:u8 = 4;
const OP_CLAIM:   u8 = 5;

// ---------- State ----------
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct VaultState {
    pub admin: Pubkey,
    pub operator: Pubkey,
    pub usdc_mint: Pubkey,
    pub share_mint: Pubkey,
    pub vault_pda: Pubkey,
    pub vault_bump: u8,
    pub _pad1: [u8; 7],
    pub total_shares: u128,
    pub pps: u128,            // fixed-point, starts at RAY
    pub buffered_base: u64,   // base USDC donated when total_shares == 0
    pub last_settle_slot: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct BoostDistributor {
    pub epoch: u64,
    pub root: [u8; 32],
    pub total_weight: u128,
    pub boost_total: u64, // total USDC allocated to boost for this epoch
    pub _pad: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ClaimBitmap256 {
    pub words: [u8; 32], // 256 claim bits
}

// ---------- Helpers ----------
fn load_mut<'a, T: Pod>(ai: &'a AccountInfo) -> Result<&'a mut T, ProgramError> {
    let data = ai.try_borrow_mut_data()?;
    if data.len() < size_of::<T>() { return Err(ProgramError::InvalidAccountData) }
    let ptr = data.as_mut_ptr();
    let slice = unsafe { core::slice::from_raw_parts_mut(ptr, size_of::<T>()) };
    Ok(bytemuck::from_bytes_mut(slice))
}

fn check_signer(ai: &AccountInfo) -> ProgramResult {
    if !ai.is_signer { return Err(ProgramError::MissingRequiredSignature) }
    Ok(())
}

fn derive_vault_pda(program_id: &Pubkey, usdc_mint: &Pubkey, admin: &Pubkey) -> (Pubkey, u8) {
    // SAFETY: use runtime syscall
    let seeds: [&[u8]; 3] = [SEED_VAULT, usdc_mint.as_ref(), admin.as_ref()];
    let mut out = Pubkey::default();
    let mut bump: u8 = 0;
    unsafe {
        syscalls::sol_try_find_program_address(&seeds, program_id, &mut out, &mut bump);
    }
    (out, bump)
}

fn keccak256(chunks: &[&[u8]], out: &mut [u8; 32]) {
    let mut total_len = 0usize;
    for c in chunks { total_len += c.len(); }
    let mut tmp = [0u8; 512]; // small stack buffer for short inputs
    let mut written = 0;
    for c in chunks {
        let n = c.len();
        tmp[written..written+n].copy_from_slice(c);
        written += n;
    }
    unsafe { syscalls::sol_keccak256(&tmp[..written], out) };
}

fn verify_merkle(root: &[u8; 32], leaf: &[u8; 32], proof: &[[u8;32]]) -> bool {
    let mut cur = *leaf;
    let mut buf = [0u8; 32];
    for node in proof {
        // sort orderless: hash(min||max)
        let (a, b) = if cur <= *node { (&cur, node) } else { (node, &cur) };
        keccak256(&[a, b], &mut buf);
        cur = buf;
    }
    &cur == root
}

// SPL Token CPI data builders (checked variants)
fn data_transfer_checked(amount: u64, decimals: u8) -> [u8; 1+8+1] {
    let mut d = [0u8; 10];
    d[0] = IX_TRANSFER_CHECKED;
    d[1..9].copy_from_slice(&amount.to_le_bytes());
    d[9] = decimals;
    d
}
fn data_mint_to_checked(amount: u64, decimals: u8) -> [u8; 1+8+1] {
    let mut d = [0u8; 10];
    d[0] = IX_MINT_TO_CHECKED;
    d[1..9].copy_from_slice(&amount.to_le_bytes());
    d[9] = decimals;
    d
}
fn data_burn_checked(amount: u64, decimals: u8) -> [u8; 1+8+1] {
    let mut d = [0u8; 10];
    d[0] = IX_BURN_CHECKED;
    d[1..9].copy_from_slice(&amount.to_le_bytes());
    d[9] = decimals;
    d
}

fn ix(program: &AccountInfo, data: Vec<u8>, metas: Vec<AccountMeta>) -> Instruction {
    Instruction { program_id: *program.key, accounts: metas, data }
}

fn vault_signer<'a>(vault_state: &VaultState) -> Signer<'a> {
    // signer seeds = [SEED_VAULT, usdc, admin, [bump]]
    let mut bump = [0u8; 1];
    bump[0] = vault_state.vault_bump;
    Signer::new(&*SEED_VAULT, &vault_state.usdc_mint.to_bytes(), &vault_state.admin.to_bytes(), &bump)
}

// ---------- Entry ----------
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    ix_data: &[u8],
) -> ProgramResult {
    if ix_data.is_empty() { return Err(ProgramError::InvalidInstructionData) }
    match ix_data[0] {
        OP_INIT    => op_init(program_id, accounts, &ix_data[1..]),
        OP_DEPOSIT => op_deposit(accounts, &ix_data[1..]),
        OP_WITHDRAW=> op_withdraw(accounts, &ix_data[1..]),
        OP_DONATE  => op_donate(accounts, &ix_data[1..]),
        OP_POSTROOT=> op_post_root(accounts, &ix_data[1..]),
        OP_CLAIM   => op_claim(accounts, &ix_data[1..]),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

// data: [decimals:u8]
fn op_init(program_id: &Pubkey, accs: &[AccountInfo], data: &[u8]) -> ProgramResult {
    // accounts:
    // 0 [w] vault_state
    // 1 []  admin (signer)
    // 2 []  operator
    // 3 []  usdc_mint
    // 4 []  share_mint
    // 5 []  vault_pda
    let [a0,a1,a2,a3,a4,a5, ..] = accs else { return Err(ProgramError::NotEnoughAccountKeys) };
    check_signer(a1)?;
    let decimals = data.get(0).ok_or(ProgramError::InvalidInstructionData)?.to_owned();
    let st = load_mut::<VaultState>(a0)?;
    let (vault_pda, bump) = derive_vault_pda(program_id, a3.key, a1.key);

    if *a5.key != vault_pda { return Err(ProgramError::InvalidSeeds) }

    *st = VaultState {
        admin: *a1.key,
        operator: *a2.key,
        usdc_mint: *a3.key,
        share_mint: *a4.key,
        vault_pda,
        vault_bump: bump,
        _pad1: [0;7],
        total_shares: 0,
        pps: RAY, // 1.0
        buffered_base: 0,
        last_settle_slot: 0,
    };

    msg!("vault initialized, decimals={}", decimals as u64);
    Ok(())
}

// data: [amount_usdc:u64, usdc_decimals:u8]
fn op_deposit(accs: &[AccountInfo], data: &[u8]) -> ProgramResult {
    // accounts:
    // 0 [w] vault_state
    // 1 []  vault_pda
    // 2 [s] user
    // 3 [w] user_usdc_ata
    // 4 [w] vault_usdc_ata
    // 5 [w] share_mint
    // 6 [w] user_share_ata
    // 7 []  token_program
    // 8 []  usdc_mint
    let [a0,a1,a2,a3,a4,a5,a6,a7,a8, ..] = accs else { return Err(ProgramError::NotEnoughAccountKeys) };
    check_signer(a2)?;
    let amount = u64::from_le_bytes(data[..8].try_into().unwrap());
    let usdc_decimals = data[8];

    let st = load_mut::<VaultState>(a0)?;
    if *a1.key != st.vault_pda { return Err(ProgramError::InvalidSeeds) }
    if *a5.key != st.share_mint || *a8.key != st.usdc_mint { return Err(ProgramError::InvalidArgument) }

    // 1) pull USDC from user -> vault ATA
    {
        let metas = vec![
            AccountMeta::new(*a3.key, true),       // src
            AccountMeta::new_readonly(*a8.key, false), // mint
            AccountMeta::new(*a4.key, false),      // dst
            AccountMeta::new_readonly(*a2.key, true),  // owner
        ];
        let data = data_transfer_checked(amount, usdc_decimals).to_vec();
        let ix = ix(a7, data, metas);
        cpi::invoke(&ix, &[a7,a3,a8,a4,a2])?;
    }

    // settle buffered if any and shares > 0
    if st.buffered_base > 0 && st.total_shares > 0 {
        let delta = ((st.buffered_base as u128) * RAY) / st.total_shares;
        st.pps = st.pps.checked_add(delta).ok_or(ProgramError::InvalidInstructionData)?;
        st.buffered_base = 0;
    }

    // 2) mint vault shares to user
    let shares = if st.total_shares == 0 {
        // first depositor: 1:1
        amount as u128 * RAY / st.pps
    } else {
        amount as u128 * RAY / st.pps
    };
    let mint_amt: u64 = shares.try_into().map_err(|_| ProgramError::InvalidInstructionData)?;
    {
        let metas = vec![
            AccountMeta::new(*a5.key, false), // mint
            AccountMeta::new(*a6.key, false), // dst
            AccountMeta::new_readonly(*a1.key, false), // mint authority (vault_pda)
        ];
        let data = data_mint_to_checked(mint_amt, 6).to_vec(); // share mint uses 6 decimals too (convention)
        let ix = ix(a7, data, metas);
        let signer = vault_signer(st);
        cpi::invoke_signed(&ix, &[a7,a5,a6,a1], &[&signer])?;
    }

    st.total_shares = st.total_shares.checked_add(shares).ok_or(ProgramError::InvalidInstructionData)?;
    Ok(())
}

// data: [shares:u64, usdc_decimals:u8]
fn op_withdraw(accs: &[AccountInfo], data: &[u8]) -> ProgramResult {
    // accounts:
    // 0 [w] vault_state
    // 1 []  vault_pda
    // 2 [s] user
    // 3 [w] user_usdc_ata
    // 4 [w] vault_usdc_ata
    // 5 [w] share_mint
    // 6 [w] user_share_ata
    // 7 []  token_program
    // 8 []  usdc_mint
    let [a0,a1,a2,a3,a4,a5,a6,a7,a8, ..] = accs else { return Err(ProgramError::NotEnoughAccountKeys) };
    check_signer(a2)?;
    let shares_burn: u64 = u64::from_le_bytes(data[..8].try_into().unwrap());
    let usdc_decimals = data[8];

    let st = load_mut::<VaultState>(a0)?;
    if *a1.key != st.vault_pda { return Err(ProgramError::InvalidSeeds) }

    // burn shares from user
    {
        let metas = vec![
            AccountMeta::new(*a6.key, false), // account to burn from
            AccountMeta::new(*a5.key, false), // share mint
            AccountMeta::new_readonly(*a2.key, true), // owner is user
        ];
        let data = data_burn_checked(shares_burn, 6).to_vec();
        let ix = ix(a7, data, metas);
        cpi::invoke(&ix, &[a7,a6,a5,a2])?;
    }

    // send USDC to user equal to shares * pps
    let shares_u128 = shares_burn as u128;
    let amount_out_u128 = shares_u128
        .checked_mul(st.pps).ok_or(ProgramError::InvalidInstructionData)?
        / RAY;
    let amount_out: u64 = amount_out_u128.try_into().map_err(|_| ProgramError::InvalidInstructionData)?;

    // transfer vault USDC -> user USDC using vault signer
    {
        let metas = vec![
            AccountMeta::new(*a4.key, false), // src vault
            AccountMeta::new_readonly(*a8.key, false), // mint
            AccountMeta::new(*a3.key, false), // dst
            AccountMeta::new_readonly(*a1.key, false), // owner vault_pda
        ];
        let data = data_transfer_checked(amount_out, usdc_decimals).to_vec();
        let ix = ix(a7, data, metas);
        let signer = vault_signer(st);
        cpi::invoke_signed(&ix, &[a7,a4,a8,a3,a1], &[&signer])?;
    }

    st.total_shares = st.total_shares.checked_sub(shares_u128).ok_or(ProgramError::InvalidInstructionData)?;
    Ok(())
}

// data: [amount_usdc:u64, epoch:u64, boost_bps:u16, usdc_decimals:u8]
fn op_donate(accs: &[AccountInfo], data: &[u8]) -> ProgramResult {
    // accounts:
    // 0 [w] vault_state
    // 1 []  vault_pda
    // 2 [s] operator (or anyone)
    // 3 [w] operator_usdc_ata
    // 4 [w] vault_usdc_ata
    // 5 [w] boost_usdc_ata   (owned by vault_pda)
    // 6 []  token_program
    // 7 []  usdc_mint
    // 8 [w] boost_distributor (for epoch)  (optional writable if present)
    let [a0,a1,a2,a3,a4,a5,a6,a7,a8, ..] = accs else { return Err(ProgramError::NotEnoughAccountKeys) };
    check_signer(a2)?;
    let amount = u64::from_le_bytes(data[0..8].try_into().unwrap());
    let epoch  = u64::from_le_bytes(data[8..16].try_into().unwrap());
    let boost_bps = u16::from_le_bytes(data[16..18].try_into().unwrap()) as u64;
    let usdc_decimals = data[18];

    let st = load_mut::<VaultState>(a0)?;
    if *a1.key != st.vault_pda { return Err(ProgramError::InvalidSeeds) }

    // operator_ata -> vault_ata
    {
        let metas = vec![
            AccountMeta::new(*a3.key, true),
            AccountMeta::new_readonly(*a7.key, false),
            AccountMeta::new(*a4.key, false),
            AccountMeta::new_readonly(*a2.key, true),
        ];
        let ix = ix(a6, data_transfer_checked(amount, usdc_decimals).to_vec(), metas);
        cpi::invoke(&ix, &[a6,a3,a7,a4,a2])?;
    }

    let boost = amount * boost_bps / 10_000;
    let base  = amount - boost;

    // vault_ata -> boost_ata (boost part) signed by vault
    if boost > 0 {
        let metas = vec![
            AccountMeta::new(*a4.key, false),
            AccountMeta::new_readonly(*a7.key, false),
            AccountMeta::new(*a5.key, false),
            AccountMeta::new_readonly(*a1.key, false),
        ];
        let ix = ix(a6, data_transfer_checked(boost, usdc_decimals).to_vec(), metas);
        let signer = vault_signer(st);
        cpi::invoke_signed(&ix, &[a6,a4,a7,a5,a1], &[&signer])?;
    }

    // bump PPS or buffer
    if st.total_shares > 0 {
        let delta = ((base as u128) * RAY) / st.total_shares;
        st.pps = st.pps.checked_add(delta).ok_or(ProgramError::InvalidInstructionData)?;
    } else {
        st.buffered_base = st.buffered_base.saturating_add(base);
    }

    // Optional: update boost distributor (if provided)
    if a8.owner == a0.owner && a8.data_len() >= size_of::<BoostDistributor>() {
        let bd = load_mut::<BoostDistributor>(a8)?;
        if bd.epoch == 0 { bd.epoch = epoch; }
        if bd.epoch != epoch { return Err(ProgramError::InvalidArgument) }
        bd.boost_total = bd.boost_total.saturating_add(boost);
    }

    Ok(())
}

// data: [epoch:u64, total_weight:u128, root: [u8;32]]
fn op_post_root(accs: &[AccountInfo], data: &[u8]) -> ProgramResult {
    // accounts:
    // 0 [w] vault_state
    // 1 []  operator (signer)
    // 2 [w] boost_distributor (PDA)
    let [a0,a1,a2, ..] = accs else { return Err(ProgramError::NotEnoughAccountKeys) };
    check_signer(a1)?;
    let _st = load_mut::<VaultState>(a0)?; // enforce ownership but we don't use fields
    let epoch = u64::from_le_bytes(data[0..8].try_into().unwrap());
    let total_weight = u128::from_le_bytes(data[8..24].try_into().unwrap());
    let mut root = [0u8;32];
    root.copy_from_slice(&data[24..56]);

    let bd = load_mut::<BoostDistributor>(a2)?;
    bd.epoch = epoch;
    bd.total_weight = total_weight;
    bd.root = root;
    Ok(())
}

// data: [epoch:u64, index:u32, weight:u128, proof_len:u8, proof_nodes... (32b each)]
fn op_claim(accs: &[AccountInfo], data: &[u8]) -> ProgramResult {
    // accounts:
    // 0 [w] vault_state
    // 1 []  vault_pda
    // 2 [s] claimer
    // 3 [w] boost_distributor
    // 4 [w] claims_bitmap
    // 5 [w] boost_usdc_ata (owned by vault_pda)
    // 6 [w] claimer_usdc_ata
    // 7 []  token_program
    // 8 []  usdc_mint
    let [a0,a1,a2,a3,a4,a5,a6,a7,a8, ..] = accs else { return Err(ProgramError::NotEnoughAccountKeys) };
    check_signer(a2)?;
    let epoch = u64::from_le_bytes(data[0..8].try_into().unwrap());
    let index = u32::from_le_bytes(data[8..12].try_into().unwrap());
    let weight = u128::from_le_bytes(data[12..28].try_into().unwrap());
    let proof_len = data[28] as usize;
    let mut off = 29usize;

    let st = load_mut::<VaultState>(a0)?;
    if *a1.key != st.vault_pda { return Err(ProgramError::InvalidSeeds) }
    let bd = load_mut::<BoostDistributor>(a3)?;
    if bd.epoch != epoch { return Err(ProgramError::InvalidArgument) }
    if bd.total_weight == 0 { return Err(ProgramError::InvalidInstructionData) }

    // bitmap
    let bm = load_mut::<ClaimBitmap256>(a4)?;
    let bit = (index & 7) as u8;
    let byte = (index / 8) as usize;
    if byte >= bm.words.len() { return Err(ProgramError::InvalidInstructionData) }
    let mask = 1u8 << bit;
    if (bm.words[byte] & mask) != 0 { return Err(ProgramError::Custom(1)) } // already claimed

    // proof
    let mut leaf = [0u8;32];
    let mut idx_le = [0u8;4];
    idx_le.copy_from_slice(&index.to_le_bytes());
    // domain-separated leaf: keccak(b"weight", index, claimer, weight)
    keccak256(&[
        b"weight",
        &idx_le,
        a2.key.as_ref(),
        &weight.to_le_bytes(),
    ], &mut leaf);

    // read proof nodes
    let nodes = proof_len;
    if data.len() < off + nodes*32 { return Err(ProgramError::InvalidInstructionData) }
    let mut proof = [[0u8;32]; 16];
    if nodes > 16 { return Err(ProgramError::InvalidInstructionData) }
    for i in 0..nodes {
        proof[i].copy_from_slice(&data[off..off+32]);
        off += 32;
    }
    let ok = verify_merkle(&bd.root, &leaf, &proof[..nodes]);
    if !ok { return Err(ProgramError::InvalidArgument) }

    // compute claim amount
    let claim_u128 = (bd.boost_total as u128)
        .saturating_mul(weight)
        / bd.total_weight;
    let claim: u64 = claim_u128.try_into().map_err(|_| ProgramError::InvalidInstructionData)?;

    // transfer boost -> claimer
    {
        let metas = vec![
            AccountMeta::new(*a5.key, false),
            AccountMeta::new_readonly(*a8.key, false),
            AccountMeta::new(*a6.key, false),
            AccountMeta::new_readonly(*a1.key, false),
        ];
        let ix = ix(a7, data_transfer_checked(claim, 6).to_vec(), metas);
        let signer = vault_signer(st);
        cpi::invoke_signed(&ix, &[a7,a5,a8,a6,a1], &[&signer])?;
    }

    // mark claimed
    bm.words[byte] |= mask;
    Ok(())
}


