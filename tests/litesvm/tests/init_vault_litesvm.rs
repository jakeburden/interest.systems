use litesvm::LiteSVM;
use solana_instruction::{Instruction, account_meta::AccountMeta};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::Transaction;

#[test]
fn init_vault_succeeds() {
    let program_id = Pubkey::new_unique();

    // Load program bytes (build the .so first via ./scripts/build-program.sh)
    // Adjust the path if needed.
    let bytes = include_bytes!("../../../programs/interest_vault/target/deploy/interest_vault.so");

    let mut svm = LiteSVM::new();
    svm.add_program(program_id, bytes);

    let admin = Keypair::new();
    let operator = Keypair::new();
    let usdc_mint = Pubkey::new_unique();
    let share_mint = Pubkey::new_unique();
    let vault_state = Keypair::new();

    // Build instruction data: [tag=INIT, decimals=6]
    let data = vec![0u8, 6u8];

    // Accounts: vault_state(w), admin(s), operator, usdc_mint, share_mint, vault_pda
    // For init test we don't need a real PDA (program checks it, so compute off-chain)
    let seeds = [b"vault".as_ref(), usdc_mint.as_ref(), admin.pubkey().as_ref()];
    let (vault_pda, _bump) = Pubkey::find_program_address(&seeds, &program_id);

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(vault_state.pubkey(), true),
            AccountMeta::new_readonly(admin.pubkey(), true),
            AccountMeta::new_readonly(operator.pubkey(), false),
            AccountMeta::new_readonly(usdc_mint, false),
            AccountMeta::new_readonly(share_mint, false),
            AccountMeta::new_readonly(vault_pda, false),
        ],
        data,
    };

    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    let blockhash = svm.latest_blockhash();
    let tx = Transaction::new(&[&admin, &vault_state], Message::new(&[ix], Some(&admin.pubkey())), blockhash);
    let res = svm.send_transaction(tx);
    assert!(res.is_ok());
}


