use litesvm::LiteSVM;
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_message::Message;
use solana_transaction::Transaction;

const LOADER: Address = Address::from_str_const("BPFLoaderUpgradeab1e11111111111111111111111");
const SYSTEM: Address = Address::from_str_const("11111111111111111111111111111111");
const RENT: Address = Address::from_str_const("SysvarRent111111111111111111111111111111111");
const CLOCK: Address = Address::from_str_const("SysvarC1ock11111111111111111111111111111111");

fn send(svm: &mut LiteSVM, payer: &Address, instructions: &[Instruction]) -> Result<(), String> {
    svm.expire_blockhash();
    let mut limit = vec![2];
    limit.extend_from_slice(&1_400_000u32.to_le_bytes());
    let mut all = vec![Instruction {
        program_id: Address::from_str_const("ComputeBudget111111111111111111111111111111"),
        accounts: vec![],
        data: limit,
    }];
    all.extend_from_slice(instructions);
    let mut tx = Transaction::new_unsigned(Message::new(&all, Some(payer)));
    tx.message.recent_blockhash = svm.latest_blockhash();
    assert_eq!(tx.message.header.num_required_signatures, 1);
    assert_eq!(tx.message.account_keys[0], *payer);
    svm.send_transaction(tx)
        .map(|_| ())
        .map_err(|error| format!("{error:#?}"))
}

#[test]
fn real_loader_deploys_seeded_accounts_and_retains_the_expected_authority() {
    // Public addresses only. This local VM disables signature cryptography;
    // the instruction signer flags and real System/Loader authorization still run.
    let mut svm = LiteSVM::new().with_mainnet_features().with_sigverify(false);
    let payer = Address::new_unique();
    svm.airdrop(&payer, 20_000_000_000).unwrap();
    let artifact_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy/burned_fun.so");
    let artifact = std::fs::read(artifact_path).expect("run contract/build.sh first");
    assert_eq!(&artifact[..4], b"\x7fELF");
    let program_seed = "program-loader-test";
    let buffer_seed = "buffer-loader-test";
    let program = Address::create_with_seed(&payer, program_seed, &LOADER).unwrap();
    let buffer = Address::create_with_seed(&payer, buffer_seed, &LOADER).unwrap();
    let (program_data, _) = Address::find_program_address(&[program.as_ref()], &LOADER);
    let create = solana_system_interface::instruction::create_account_with_seed(
        &payer,
        &buffer,
        &payer,
        buffer_seed,
        svm.minimum_balance_for_rent_exemption(37 + artifact.len()),
        (37 + artifact.len()) as u64,
        &LOADER,
    );
    let initialize = Instruction {
        program_id: LOADER,
        accounts: vec![
            AccountMeta::new(buffer, false),
            AccountMeta::new_readonly(payer, false),
        ],
        data: 0u32.to_le_bytes().to_vec(),
    };
    send(&mut svm, &payer, &[create, initialize]).unwrap();
    for (index, chunk) in artifact.chunks(900).enumerate() {
        let mut data = 1u32.to_le_bytes().to_vec();
        data.extend_from_slice(&((index * 900) as u32).to_le_bytes());
        data.extend_from_slice(&(chunk.len() as u64).to_le_bytes());
        data.extend_from_slice(chunk);
        send(
            &mut svm,
            &payer,
            &[Instruction {
                program_id: LOADER,
                accounts: vec![
                    AccountMeta::new(buffer, false),
                    AccountMeta::new_readonly(payer, true),
                ],
                data,
            }],
        )
        .unwrap();
    }
    assert_eq!(&svm.get_account(&buffer).unwrap().data[37..], &artifact);
    let create_program = solana_system_interface::instruction::create_account_with_seed(
        &payer,
        &program,
        &payer,
        program_seed,
        svm.minimum_balance_for_rent_exemption(36),
        36,
        &LOADER,
    );
    let mut deploy_data = 2u32.to_le_bytes().to_vec();
    deploy_data.extend_from_slice(&(artifact.len() as u64).to_le_bytes());
    let deploy = Instruction {
        program_id: LOADER,
        accounts: vec![
            AccountMeta::new(payer, true),
            AccountMeta::new(program_data, false),
            AccountMeta::new(program, false),
            AccountMeta::new(buffer, false),
            AccountMeta::new_readonly(RENT, false),
            AccountMeta::new_readonly(CLOCK, false),
            AccountMeta::new_readonly(SYSTEM, false),
            AccountMeta::new_readonly(payer, true),
        ],
        data: deploy_data,
    };
    let wallet_before = svm.get_account(&payer).unwrap().lamports;
    let expected_debit = 5_000
        + svm.minimum_balance_for_rent_exemption(36)
        + svm.minimum_balance_for_rent_exemption(45 + artifact.len())
        - svm.get_account(&buffer).unwrap().lamports;
    send(&mut svm, &payer, &[create_program, deploy]).unwrap();
    assert_eq!(
        wallet_before - svm.get_account(&payer).unwrap().lamports,
        expected_debit,
        "deployment reuses buffer rent; do not charge it twice in the wallet budget"
    );
    let code = svm.get_account(&program_data).unwrap();
    assert_eq!(code.owner, LOADER);
    assert_eq!(code.data.len(), artifact.len() + 45);
    assert_eq!(&code.data[..4], &3u32.to_le_bytes());
    assert_eq!(code.data[12], 1, "upgrade authority must remain present");
    assert_eq!(&code.data[13..45], payer.as_ref());
    assert_eq!(&code.data[45..], &artifact);
    let executable = svm.get_account(&program).unwrap();
    assert!(executable.executable);
    assert_eq!(&executable.data[4..36], program_data.as_ref());
    assert!(
        svm.get_account(&buffer).is_none(),
        "buffer rent is reclaimed during deploy"
    );
    let attacker = Address::new_unique();
    svm.airdrop(&attacker, 20_000_000_000).unwrap();
    let attacker_buffer = Address::new_unique();
    let mut account = svm.get_account(&attacker).unwrap();
    account.owner = LOADER;
    account.lamports = svm.minimum_balance_for_rent_exemption(37 + artifact.len());
    account.data = vec![0; 37 + artifact.len()];
    account.data[..4].copy_from_slice(&1u32.to_le_bytes());
    account.data[4] = 1;
    account.data[5..37].copy_from_slice(attacker.as_ref());
    account.data[37..].copy_from_slice(&artifact);
    svm.set_account(attacker_buffer, account).unwrap();
    let slot = svm.get_sysvar::<solana_clock::Clock>().slot;
    svm.warp_to_slot(slot + 1);
    let upgrade = Instruction {
        program_id: LOADER,
        accounts: vec![
            AccountMeta::new(program_data, false),
            AccountMeta::new(program, false),
            AccountMeta::new(attacker_buffer, false),
            AccountMeta::new(attacker, false),
            AccountMeta::new_readonly(RENT, false),
            AccountMeta::new_readonly(CLOCK, false),
            AccountMeta::new_readonly(attacker, true),
        ],
        data: 3u32.to_le_bytes().to_vec(),
    };
    let code_before = svm.get_account(&program_data).unwrap();
    let error = send(&mut svm, &attacker, &[upgrade]).unwrap_err();
    assert!(error.contains("IncorrectAuthority"), "{error}");
    assert_eq!(svm.get_account(&program_data).unwrap(), code_before);
}
