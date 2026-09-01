#[allow(dead_code)]
#[path = "../src/identity.rs"]
mod identity;
#[allow(dead_code)]
#[path = "../src/validation.rs"]
mod validation;
use litesvm::LiteSVM;
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_message::Message;
use solana_transaction::Transaction;
use validation::*;

fn derive(seeds: &[&[u8]], program: &Address) -> Address {
    pda(seeds, program).unwrap().0
}
fn ata(owner: &Address, mint: &Address, token: &Address) -> Address {
    derive(
        &[owner.as_ref(), token.as_ref(), mint.as_ref()],
        &ASSOCIATED_TOKEN,
    )
}

fn send(svm: &mut LiteSVM, payer: &Address, ix: Instruction) -> Result<u64, String> {
    send_many(svm, payer, &[ix])
}

fn send_many(
    svm: &mut LiteSVM,
    payer: &Address,
    instructions: &[Instruction],
) -> Result<u64, String> {
    svm.expire_blockhash();
    let mut limit = vec![2];
    limit.extend_from_slice(&1_400_000u32.to_le_bytes());
    let compute = Instruction {
        program_id: Address::from_str_const("ComputeBudget111111111111111111111111111111"),
        accounts: vec![],
        data: limit,
    };
    let mut all = vec![compute];
    all.extend_from_slice(instructions);
    let mut tx = Transaction::new_unsigned(Message::new(&all, Some(payer)));
    tx.message.recent_blockhash = svm.latest_blockhash();
    svm.send_transaction(tx)
        .map(|metadata| metadata.compute_units_consumed)
        .map_err(|error| format!("{:?}\n{}", error.err, error.meta.logs.join("\n")))
}

struct Fixture {
    svm: LiteSVM,
    program: Address,
    payer: Address,
    keys: Vec<Address>,
}

impl Fixture {
    fn new() -> Self {
        Self::with_program(Address::new_unique())
    }

    fn with_program(program: Address) -> Self {
        // Public fixture addresses only; no private keys are created or used.
        // Signature verification is disabled exclusively in the local VM.
        let mut svm = LiteSVM::new().with_mainnet_features().with_sigverify(false);
        let payer = identity::AUTHORITY;
        svm.airdrop(&payer, 10_000_000_000).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy");
        svm.add_program_from_file(program.clone(), root.join("burned_fun.so"))
            .unwrap();
        for id in [&PUMP, &AMM, &FEES, &MAYHEM] {
            svm.add_program_from_file(id.clone(), root.join("mock_venue.so"))
                .unwrap();
        }
        let mint = identity::MINT;
        let treasury = derive(&[b"treasury", mint.as_ref()], &program);
        let curve = derive(&[b"bonding-curve", mint.as_ref()], &PUMP);
        let sharing = derive(&[b"sharing-config", mint.as_ref()], &FEES);
        let pump_vault = derive(&[b"creator-vault", sharing.as_ref()], &PUMP);
        let amm_vault = derive(&[b"creator_vault", sharing.as_ref()], &AMM);
        let mayhem_vault = derive(&[b"sol-vault"], &MAYHEM);
        let keys = vec![
            payer.clone(),
            derive(&[b"buyback", mint.as_ref()], &program),
            treasury.clone(),
            mint.clone(),
            ata(&treasury, &mint, &TOKEN_2022),
            ata(&treasury, &WSOL, &TOKEN),
            program.clone(),
            derive(&[program.as_ref()], &UPGRADEABLE_LOADER),
            pinocchio_system::ID,
            TOKEN_2022,
            TOKEN,
            ASSOCIATED_TOKEN,
            WSOL,
            PUMP,
            derive(&[b"global"], &PUMP),
            derive(&[b"mint-authority"], &PUMP),
            curve.clone(),
            ata(&curve, &mint, &TOKEN_2022),
            derive(&[b"__event_authority"], &PUMP),
            MAYHEM,
            derive(&[b"global-params"], &MAYHEM),
            mayhem_vault.clone(),
            derive(&[b"mayhem-state", mint.as_ref()], &MAYHEM),
            ata(&mayhem_vault, &mint, &TOKEN_2022),
            FEES,
            derive(&[b"__event_authority"], &FEES),
            sharing,
            pump_vault.clone(),
            ata(&pump_vault, &WSOL, &TOKEN),
            AMM,
            derive(&[b"__event_authority"], &AMM),
            amm_vault.clone(),
            ata(&amm_vault, &WSOL, &TOKEN),
            derive(&[b"user_volume_accumulator", treasury.as_ref()], &PUMP),
            derive(&[b"user_volume_accumulator", treasury.as_ref()], &AMM),
        ];
        let mut global = svm.get_account(&payer).unwrap();
        global.data = vec![0; 1000];
        global.data[..8].copy_from_slice(&[167, 232, 232, 177, 200, 108, 114, 127]);
        global.data[8] = 1;
        global.owner = PUMP;
        global.lamports = svm.minimum_balance_for_rent_exemption(1000);
        svm.set_account(keys[14].clone(), global).unwrap();
        let mut native_mint = svm.get_account(&payer).unwrap();
        native_mint.data = vec![0; 82];
        native_mint.data[44] = 9;
        native_mint.data[45] = 1;
        native_mint.owner = TOKEN;
        native_mint.lamports = svm.minimum_balance_for_rent_exemption(82);
        svm.set_account(WSOL, native_mint).unwrap();
        Self {
            svm,
            payer,
            program,
            keys,
        }
    }

    fn mapped_ix(
        &self,
        program_id: Address,
        indices: &[usize],
        writable: &[usize],
        signers: &[usize],
        data: Vec<u8>,
    ) -> Instruction {
        Instruction {
            program_id,
            accounts: indices
                .iter()
                .enumerate()
                .map(|(index, key)| AccountMeta {
                    pubkey: self.keys[*key].clone(),
                    is_writable: writable.contains(&index),
                    is_signer: signers.contains(&index),
                })
                .collect(),
            data,
        }
    }

    fn begin(&self) -> Instruction {
        self.mapped_ix(
            self.program.clone(),
            &[0, 1, 2, 3, 8, 6, 7],
            &[0, 1, 2],
            &[0],
            vec![0],
        )
    }

    fn finish(&self) -> Instruction {
        self.mapped_ix(
            self.program.clone(),
            &[0, 1, 2, 3, 4, 5, 9, 8, 26, 6, 7, 16, 27, 33, 34],
            &[0, 1, 2],
            &[0],
            vec![2],
        )
    }

    fn create_ata(&self, account: usize, owner: usize, mint: usize, token: usize) -> Instruction {
        self.mapped_ix(
            ASSOCIATED_TOKEN,
            &[0, account, owner, mint, 8, token],
            &[0, 1],
            &[0],
            vec![1],
        )
    }

    fn setup(&self) -> Vec<Instruction> {
        // Same pinned Pump SDK ABIs as the operator transaction. Unlike the
        // former factory CPI, the externally reserved mint is a top-level signer.
        let mut create_data = vec![214, 144, 76, 236, 95, 139, 49, 180];
        for text in ["Burned", "BURN", "https://example.test/token.json"] {
            create_data.extend_from_slice(&(text.len() as u32).to_le_bytes());
            create_data.extend_from_slice(text.as_bytes());
        }
        create_data.extend_from_slice(self.payer.as_ref());
        create_data.extend_from_slice(&[0, 0]);
        let create = self.mapped_ix(
            PUMP,
            &[3, 15, 16, 17, 14, 0, 8, 9, 11, 19, 20, 21, 22, 23, 18, 13],
            &[0, 2, 3, 5, 9, 11, 12, 13],
            &[0, 5],
            create_data,
        );
        let create_sharing = self.mapped_ix(
            FEES,
            &[25, 24, 0, 14, 3, 26, 8, 16, 13, 18, 24, 29, 30],
            &[2, 5, 7],
            &[2],
            vec![195, 78, 86, 76, 111, 52, 251, 213],
        );
        let extend = self.mapped_ix(
            PUMP,
            &[16, 0, 8, 18, 13],
            &[0],
            &[1],
            vec![234, 102, 194, 203, 150, 72, 62, 229],
        );
        let mut shares = vec![111, 251, 49, 6, 78, 78, 106, 18];
        shares.extend_from_slice(&1u32.to_le_bytes());
        shares.extend_from_slice(self.keys[2].as_ref());
        shares.extend_from_slice(&10_000u16.to_le_bytes());
        let lock = self.mapped_ix(
            FEES,
            &[
                25, 24, 0, 14, 3, 26, 16, 27, 28, 8, 13, 18, 29, 30, 12, 10, 11, 31, 32, 0,
            ],
            &[2, 5, 7, 8, 17, 18, 19],
            &[2],
            shares,
        );
        let mut result = vec![
            self.begin(),
            create,
            extend,
            create_sharing,
            lock,
            self.create_ata(4, 2, 3, 9),
            self.create_ata(5, 2, 12, 10),
            self.create_ata(32, 31, 12, 10),
        ];
        let rent = self.svm.minimum_balance_for_rent_exemption(0);
        let current = self
            .svm
            .get_account(&self.keys[27])
            .map_or(0, |a| a.lamports);
        for (account, event, program) in [(33, 18, 13), (34, 30, 29)] {
            result.push(self.mapped_ix(
                self.keys[program].clone(),
                &[0, 2, account, 8, event, program],
                &[0, 2],
                &[0],
                vec![94, 6, 202, 115, 255, 96, 232, 183],
            ));
        }
        result.push(solana_system_interface::instruction::transfer(
            &self.payer,
            &self.keys[27],
            rent.saturating_sub(current),
        ));
        result.push(self.finish());
        result
    }

    fn mode(&mut self, value: u8) {
        let mut global = self.svm.get_account(&self.keys[14]).unwrap();
        global.data[999] = value;
        self.svm.set_account(self.keys[14].clone(), global).unwrap();
    }

    fn rejects_atomically(&mut self, instructions: &[Instruction]) -> String {
        let indices = [1, 2, 3, 4, 5, 16, 17, 26, 27, 28, 32, 33, 34];
        let before: Vec<_> = indices
            .iter()
            .map(|index| self.svm.get_account(&self.keys[*index]))
            .collect();
        let error = send_many(&mut self.svm, &self.payer, instructions).unwrap_err();
        for (index, previous) in indices.iter().zip(before) {
            assert_eq!(
                self.svm.get_account(&self.keys[*index]),
                previous,
                "atomic rollback index {index}"
            );
        }
        error
    }
}

#[test]
fn outer_sdk_setup_creates_the_fixed_token_and_locks_all_fees_atomically() {
    let mut f = Fixture::new();
    let before = f.svm.get_account(&f.payer).unwrap().lamports;
    let instructions = f.setup();
    let message = Message::new(&instructions, Some(&f.payer));
    assert_eq!(message.header.num_required_signatures, 2);
    assert_eq!(
        &message.account_keys[..2],
        &[identity::AUTHORITY, identity::MINT]
    );
    let units = send_many(&mut f.svm, &f.payer, &instructions).unwrap();
    println!("Local mock-venue atomic SDK setup: {units} compute units");
    let mint = f.svm.get_account(&f.keys[3]).unwrap();
    assert_eq!(mint.owner, TOKEN_2022);
    assert_eq!(validate_mint_data(&mint.data).unwrap(), 6);
    assert!(u64_at(&mint.data, 36).unwrap() > 0);
    let sharing = f.svm.get_account(&f.keys[26]).unwrap();
    validate_sharing_data(&sharing.data, &f.keys[3], &f.keys[2]).unwrap();
    assert_eq!(sharing.owner, FEES);
    let curve = f.svm.get_account(&f.keys[16]).unwrap();
    assert!(address_at(&curve.data, 49, &f.keys[26]));
    let config = f.svm.get_account(&f.keys[1]).unwrap();
    assert_eq!(config.owner, f.program);
    assert_eq!(&config.data[..8], STATE_TAG);
    assert!(address_at(&config.data, 8, &f.keys[3]));
    assert_eq!(config.data[74], 6);
    assert_eq!(config.data[75], 1);
    assert_eq!(&config.data[80..112], &[0; 32]);
    for (index, owner) in [
        (4, &TOKEN_2022),
        (5, &TOKEN),
        (27, &pinocchio_system::ID),
        (32, &TOKEN),
        (33, &PUMP),
        (34, &AMM),
    ] {
        let account = f.svm.get_account(&f.keys[index]).unwrap();
        assert_eq!(&account.owner, owner);
        assert!(account.lamports >= f.svm.minimum_balance_for_rent_exemption(account.data.len()));
    }
    let debit = before - f.svm.get_account(&f.payer).unwrap().lamports;
    assert!(debit <= 100_000_000 + 10_000); // Two external signatures' network fees.
    let begin_again = f.begin();
    f.rejects_atomically(&[begin_again]);
    let finish_again = f.finish();
    f.rejects_atomically(&[finish_again]);
}

#[test]
fn final_activation_rolls_back_mint_fees_and_rent_for_bad_routes_or_setup_debits() {
    for mode in [1, 2, 3] {
        let mut f = Fixture::new();
        f.mode(mode); // Wrong recipient, retained admin, or upstream failure.
        let instructions = f.setup();
        f.rejects_atomically(&instructions);
    }
    let mut f = Fixture::new();
    let mut instructions = f.setup();
    instructions.remove(2); // Without SDK extension the curve remains 115 bytes.
    assert!(f.rejects_atomically(&instructions).contains("Custom(6005)"));
    let mut f = Fixture::new();
    let mut bad_share = f.setup();
    bad_share[4].data[44..46].copy_from_slice(&9_999u16.to_le_bytes());
    assert!(f.rejects_atomically(&bad_share).contains("Custom(6002)"));
    let mut f = Fixture::new();
    let mut instructions = f.setup();
    instructions.insert(
        instructions.len() - 1,
        solana_system_interface::instruction::transfer(
            &f.payer,
            &Address::new_unique(),
            100_000_000,
        ),
    );
    assert!(f.rejects_atomically(&instructions).contains("Custom(6013)"));
}

#[test]
fn creator_vault_donations_do_not_block_atomic_activation() {
    let mut f = Fixture::new();
    f.svm.airdrop(&f.keys[27], 200_000_000).unwrap();
    let before = f.svm.get_account(&f.payer).unwrap().lamports;
    let instructions = f.setup();
    send_many(&mut f.svm, &f.payer, &instructions).unwrap();
    assert!(f.svm.get_account(&f.payer).unwrap().lamports > before);
    let state = f.svm.get_account(&f.keys[1]).unwrap();
    assert_eq!(state.data[75], 1);
    assert_eq!(&state.data[80..112], &[0; 32]);
}

#[test]
fn setup_accepts_only_the_compiled_upgrade_authority() {
    let mut accepted = Fixture::new();
    let mut program_data = accepted.svm.get_account(&accepted.keys[7]).unwrap();
    program_data.data[12] = 1;
    program_data.data[13..45].copy_from_slice(accepted.payer.as_ref());
    accepted
        .svm
        .set_account(accepted.keys[7].clone(), program_data)
        .unwrap();
    let instructions = accepted.setup();
    send_many(&mut accepted.svm, &accepted.payer, &instructions).unwrap();
    assert_eq!(accepted.svm.get_account(&accepted.keys[1]).unwrap().data[75], 1);

    let mut rejected = Fixture::new();
    let mut program_data = rejected.svm.get_account(&rejected.keys[7]).unwrap();
    program_data.data[12] = 1;
    program_data.data[13..45].copy_from_slice(Address::new_unique().as_ref());
    rejected
        .svm
        .set_account(rejected.keys[7].clone(), program_data)
        .unwrap();
    let instructions = rejected.setup();
    assert!(rejected
        .rejects_atomically(&instructions)
        .contains("Custom(6011)"));
}

#[test]
fn setup_refuses_wrong_identities_and_missing_mint_signature() {
    let mut f = Fixture::new();
    for index in [1, 2, 3, 4, 5, 6] {
        let mut instructions = f.setup();
        instructions[0].accounts[index].pubkey = f.payer.clone();
        f.rejects_atomically(&instructions);
    }
    for index in [0, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14] {
        let mut instructions = f.setup();
        instructions.last_mut().unwrap().accounts[index].pubkey = Address::new_unique();
        f.rejects_atomically(&instructions);
    }
    let mut wrong_authority = f.setup();
    wrong_authority[0].accounts[0] = AccountMeta::new(Address::new_unique(), true);
    f.rejects_atomically(&wrong_authority);
    let mut no_authority_signature = f.setup();
    no_authority_signature[0].accounts[0] = AccountMeta::new(Address::new_unique(), false);
    f.rejects_atomically(&no_authority_signature);
    let mut no_mint_signature = f.setup();
    no_mint_signature[1].accounts[0].is_signer = false;
    f.rejects_atomically(&no_mint_signature);

    let mut existing = Fixture::new();
    let mut mint = existing.svm.get_account(&existing.payer).unwrap();
    mint.owner = TOKEN_2022;
    mint.data = vec![0; 82];
    mint.data[44] = 6;
    mint.data[45] = 1;
    mint.lamports = existing.svm.minimum_balance_for_rent_exemption(82);
    existing
        .svm
        .set_account(existing.keys[3].clone(), mint)
        .unwrap();
    let begin = existing.begin();
    assert!(existing
        .rejects_atomically(&[begin])
        .contains("IllegalOwner"));
}

#[test]
fn finish_rechecks_upgrade_authority_and_cannot_activate_without_begin() {
    let mut f = Fixture::new();
    let finish = f.finish();
    f.rejects_atomically(&[finish]);
    let mut expected = f.svm.get_account(&f.keys[7]).unwrap();
    expected.data[12] = 1;
    expected.data[13..45].copy_from_slice(f.payer.as_ref());
    f.svm.set_account(f.keys[7].clone(), expected.clone()).unwrap();
    let mut pending = f.setup();
    pending.pop();
    send_many(&mut f.svm, &f.payer, &pending).unwrap();
    let state = f.svm.get_account(&f.keys[1]).unwrap();
    assert_eq!(state.data[75], 0);
    assert!(u64_at(&state.data, 104).unwrap() > f.svm.get_account(&f.payer).unwrap().lamports);
    let mut wrong = expected.clone();
    wrong.data[13..45].copy_from_slice(Address::new_unique().as_ref());
    f.svm.set_account(f.keys[7].clone(), wrong).unwrap();
    let finish = f.finish();
    assert!(f.rejects_atomically(&[finish]).contains("Custom(6011)"));
    f.svm.set_account(f.keys[7].clone(), expected).unwrap();
    let finish = f.finish();
    send(&mut f.svm, &f.payer, finish).unwrap();
    assert_eq!(f.svm.get_account(&f.keys[1]).unwrap().data[75], 1);
}

fn mainnet_fixture() -> Fixture {
    use std::str::FromStr;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/mainnet-clone");
    let mut f = Fixture::with_program(Address::from_str_const(
        "C1FqoguM1WredxG2SG6HfTAKwiby4iG2bZ41KSMbBURN",
    ));
    for row in std::fs::read_to_string(root.join("programs.tsv"))
        .expect("fetch public snapshot first")
        .lines()
    {
        let fields: Vec<_> = row.split('\t').collect();
        let address = Address::from_str(fields[0]).unwrap();
        let loader = Address::from_str(fields[1]).unwrap();
        let bytes = std::fs::read(root.join(fields[2])).unwrap();
        f.svm
            .add_program_with_loader(address, &bytes, loader)
            .unwrap();
    }
    for row in std::fs::read_to_string(root.join("accounts.tsv"))
        .unwrap()
        .lines()
    {
        let fields: Vec<_> = row.split('\t').collect();
        let mut account = f.svm.get_account(&f.payer).unwrap();
        account.owner = Address::from_str(fields[1]).unwrap();
        account.lamports = fields[2].parse().unwrap();
        account.executable = fields[3] == "true";
        account.data = std::fs::read(root.join(fields[4])).unwrap();
        f.svm
            .set_account(Address::from_str(fields[0]).unwrap(), account)
            .unwrap();
    }
    f
}

#[test]
#[ignore = "Run node contract/fetch-mainnet.mjs first; public snapshot compatibility, not a chain transaction"]
fn real_mainnet_programs_launch_distribute_fees_buy_and_burn_in_local_litesvm() {
    let mut f = mainnet_fixture();
    let before = f.svm.get_account(&f.payer).unwrap().lamports;
    let instructions = f.setup();
    assert_eq!(
        Message::new(&instructions, Some(&f.payer))
            .header
            .num_required_signatures,
        2
    );
    let units = send_many(&mut f.svm, &f.payer, &instructions).unwrap();
    let mint = f.svm.get_account(&f.keys[3]).unwrap();
    assert_eq!(mint.owner, TOKEN_2022);
    validate_mint_data(&mint.data).unwrap();
    let sharing = f.svm.get_account(&f.keys[26]).unwrap();
    validate_sharing_data(&sharing.data, &f.keys[3], &f.keys[2]).unwrap();
    assert_eq!(sharing.owner, FEES);
    assert!(address_at(
        &f.svm.get_account(&f.keys[16]).unwrap().data,
        49,
        &f.keys[26]
    ));
    assert_eq!(&f.svm.get_account(&f.keys[1]).unwrap().data[..8], STATE_TAG);
    assert_eq!(f.svm.get_account(&f.keys[1]).unwrap().data[75], 1);
    assert_eq!(f.svm.get_account(&f.keys[16]).unwrap().data.len(), 151);
    assert_eq!(f.svm.get_account(&f.keys[32]).unwrap().owner, TOKEN);
    let debit = before - f.svm.get_account(&f.payer).unwrap().lamports;
    println!("Real mainnet program snapshot, local VM only: launch+permanent100%feeRoute PASS; CU={units}; debit={debit}lamports including signaturefee");

    // Bootstrap only the local treasury with a public wallet's simulated SOL.
    // A separate wallet pays for every permissionless trigger below.
    let anyone = Address::new_unique();
    f.svm.airdrop(&anyone, 10_000_000).unwrap();
    let funding = solana_system_interface::instruction::transfer(&f.payer, &f.keys[2], 100_000_000);
    send(&mut f.svm, &f.payer, funding).unwrap();
    let global = f.svm.get_account(&f.keys[14]).unwrap();
    let recipient = Address::new_from_array(global.data[41..73].try_into().unwrap());
    let buyback = Address::new_from_array(global.data[741..773].try_into().unwrap());
    let pump_keys = vec![
        f.keys[14].clone(),
        recipient,
        f.keys[3].clone(),
        f.keys[16].clone(),
        f.keys[17].clone(),
        f.keys[4].clone(),
        f.keys[2].clone(),
        pinocchio_system::ID,
        TOKEN_2022,
        f.keys[27].clone(),
        f.keys[18].clone(),
        PUMP,
        derive(&[b"global_volume_accumulator"], &PUMP),
        f.keys[33].clone(),
        derive(&[b"fee_config", PUMP.as_ref()], &FEES),
        FEES,
        derive(&[b"bonding-curve-v2", f.keys[3].as_ref()], &PUMP),
        buyback,
    ];
    let mut accounts: Vec<_> = [1, 2, 3, 4, 5, 26, 9, 8]
        .into_iter()
        .enumerate()
        .map(|(index, key)| AccountMeta {
            pubkey: f.keys[key].clone(),
            is_writable: index < 5,
            is_signer: false,
        })
        .collect();
    accounts.extend(
        pump_keys
            .into_iter()
            .enumerate()
            .map(|(index, pubkey)| AccountMeta {
                pubkey,
                is_writable: [1, 3, 4, 5, 6, 9, 13, 17].contains(&index),
                is_signer: false,
            }),
    );
    let mut data = vec![1, 0];
    data.extend_from_slice(&1u64.to_le_bytes()); // On-chain reserve floor remains mandatory.
    data.extend_from_slice(&(f.svm.get_sysvar::<solana_clock::Clock>().slot + 150).to_le_bytes());
    let buy_burn = Instruction {
        program_id: f.program.clone(),
        accounts,
        data,
    };
    let supply_before = u64_at(&mint.data, 36).unwrap();
    let treasury_before = f.svm.get_account(&f.keys[2]).unwrap().lamports;
    let first_units = send(&mut f.svm, &anyone, buy_burn.clone()).unwrap();
    let state = f.svm.get_account(&f.keys[1]).unwrap();
    let spent = u64_at(&state.data, 80).unwrap();
    let burned = u64_at(&state.data, 88).unwrap();
    assert!(spent > 0 && spent <= 100_000_000);
    assert!(burned > 0);
    assert_eq!(u64_at(&state.data, 96).unwrap(), 1);
    assert_eq!(
        treasury_before - f.svm.get_account(&f.keys[2]).unwrap().lamports,
        spent
    );
    assert_eq!(
        u64_at(&f.svm.get_account(&f.keys[4]).unwrap().data, 64).unwrap(),
        0
    );
    assert_eq!(
        supply_before - u64_at(&f.svm.get_account(&f.keys[3]).unwrap().data, 36).unwrap(),
        burned
    );

    // The real buy generated real creator fees in the canonical creator vault.
    // Match the site's collect+execute transaction: distribute every available
    // creator-fee lamport to treasury, then use it to buy and burn again.
    let reserve = f.svm.minimum_balance_for_rent_exemption(0);
    let distributable = f.svm.get_account(&f.keys[27]).unwrap().lamports - reserve;
    assert!(distributable > 0);
    let distribution = Instruction {
        program_id: PUMP,
        accounts: [3, 16, 26, 27, 8, 18, 13, 2]
            .into_iter()
            .enumerate()
            .map(|(index, key)| AccountMeta {
                pubkey: f.keys[key].clone(),
                is_writable: index == 3 || index == 7,
                is_signer: false,
            })
            .collect(),
        data: vec![165, 114, 103, 0, 121, 206, 247, 81],
    };
    let treasury_before = f.svm.get_account(&f.keys[2]).unwrap().lamports;
    let second_units = send_many(&mut f.svm, &anyone, &[distribution, buy_burn]).unwrap();
    let state = f.svm.get_account(&f.keys[1]).unwrap();
    let spent_total = u64_at(&state.data, 80).unwrap();
    let burned_total = u64_at(&state.data, 88).unwrap();
    assert_eq!(u64_at(&state.data, 96).unwrap(), 2);
    assert_eq!(
        spent_total - spent,
        treasury_before + distributable - f.svm.get_account(&f.keys[2]).unwrap().lamports
    );
    assert!(spent_total - spent >= distributable - 1);
    assert!(burned_total > burned);
    assert_eq!(
        u64_at(&f.svm.get_account(&f.keys[4]).unwrap().data, 64).unwrap(),
        0
    );
    assert_eq!(
        supply_before - u64_at(&f.svm.get_account(&f.keys[3]).unwrap().data, 36).unwrap(),
        burned_total
    );
    assert!(f.svm.get_account(&f.keys[2]).unwrap().lamports >= reserve);
    println!("Real Pump+Token2022 local VM: buy/burn CU={first_units}; actual creator fees={distributable}; distribute+buy/burn CU={second_units}; total spent={spent_total}; raw supply burned={burned_total}");

    // A public address can receive donations before mint creation. The real
    // fee update sweeps these to its initial creator. A net refund must not
    // make the fixed coin permanently impossible to activate.
    let mut donated = mainnet_fixture();
    donated.svm.airdrop(&donated.keys[27], 200_000_000).unwrap();
    let before = donated.svm.get_account(&donated.payer).unwrap().lamports;
    let instructions = donated.setup();
    let units = send_many(&mut donated.svm, &donated.payer, &instructions).unwrap();
    let refund = donated.svm.get_account(&donated.payer).unwrap().lamports - before;
    assert!(refund > 0);
    let state = donated.svm.get_account(&donated.keys[1]).unwrap();
    assert_eq!(state.data[75], 1);
    assert_eq!(&state.data[80..112], &[0; 32]);
    assert_eq!(
        donated.svm.get_account(&donated.keys[27]).unwrap().lamports,
        reserve
    );
    println!("Real native creator-vault donation: atomic activation PASS; CU={units}; wallet net refund={refund}lamports");
}

fn public_instruction(
    program_id: Address,
    keys: &[Address],
    writable: &[usize],
    signers: &[usize],
    data: Vec<u8>,
) -> Instruction {
    Instruction {
        program_id,
        accounts: keys
            .iter()
            .enumerate()
            .map(|(index, pubkey)| AccountMeta {
                pubkey: *pubkey,
                is_writable: writable.contains(&index),
                is_signer: signers.contains(&index),
            })
            .collect(),
        data,
    }
}

#[test]
#[ignore = "Public snapshot required; exact production ELF, local VM only, no real signing"]
fn real_mainnet_atomic_launch_then_two_sol_authority_purchase() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(
        !cfg!(feature = "test-fixture"),
        "This rehearsal binds the production public identities"
    );
    assert_eq!(
        std::fs::read(root.join("../../target/deploy/burned_fun.so")).unwrap(),
        std::fs::read(root.join("../../../public/program/burned_fun.so")).unwrap(),
    );
    let mut f = mainnet_fixture();
    let authority_base = ata(&f.payer, &f.keys[3], &TOKEN_2022);
    let authority_volume = derive(&[b"user_volume_accumulator", f.payer.as_ref()], &PUMP);
    let mut instructions = f.setup();
    // Match the final user-approved metadata, not the shorter mock label.
    let mut create = vec![214, 144, 76, 236, 95, 139, 49, 180];
    for text in [
        "Smart Burn",
        "BURN",
        "https://ipfs.io/ipfs/bafkreidwarjuqj2c2y5wzo5qgb6b4k3fiaxtg5m6pggpmsczh5krumriye",
    ] {
        create.extend_from_slice(&(text.len() as u32).to_le_bytes());
        create.extend_from_slice(text.as_bytes());
    }
    create.extend_from_slice(f.payer.as_ref());
    create.extend_from_slice(&[0, 0]);
    instructions[1].data = create;
    // Precreate all three mint-independent volume PDAs in a separate public
    // setup transaction. Keeping them here would exceed the runtime's 64-entry
    // instruction trace limit once the initial purchase's CPIs are included.
    let volume_tag = [94, 6, 202, 115, 255, 96, 232, 183];
    let mut volumes: Vec<_> = instructions
        .iter()
        .filter(|ix| ix.data == volume_tag)
        .cloned()
        .collect();
    assert_eq!(volumes.len(), 2);
    instructions.retain(|ix| ix.data != volume_tag);
    volumes.push(public_instruction(
        PUMP,
        &[
            f.payer,
            f.payer,
            authority_volume,
            pinocchio_system::ID,
            f.keys[18],
            PUMP,
        ],
        &[0, 2],
        &[0],
        volume_tag.to_vec(),
    ));
    let volume_preparation_before = f.svm.get_account(&f.payer).unwrap().lamports;
    send_many(&mut f.svm, &f.payer, &volumes).unwrap();
    let volume_preparation_debit =
        volume_preparation_before - f.svm.get_account(&f.payer).unwrap().lamports;
    let mut price_data = vec![3];
    price_data.extend_from_slice(&1_000u64.to_le_bytes());
    // send_many already inserts the 1.4m CU limit. Include the real launch's
    // second compute-budget instruction too, without raising any trace limit.
    instructions.insert(
        0,
        public_instruction(
            Address::from_str_const("ComputeBudget111111111111111111111111111111"),
            &[],
            &[],
            &[],
            price_data,
        ),
    );
    let finish_index = instructions.len() - 1;
    assert_eq!(instructions[finish_index].data, vec![2]);
    instructions.push(public_instruction(
        ASSOCIATED_TOKEN,
        &[
            f.payer,
            authority_base,
            f.payer,
            f.keys[3],
            pinocchio_system::ID,
            TOKEN_2022,
        ],
        &[0, 1],
        &[0],
        vec![1],
    ));

    // Counterfactual preparation has identical rent and two signature fees;
    // subtracting it isolates the actual buy debit from account preparation.
    let mut prepared_only = mainnet_fixture();
    send_many(&mut prepared_only.svm, &f.payer, &volumes).unwrap();
    let preparation_before = prepared_only.svm.get_account(&f.payer).unwrap().lamports;
    send_many(&mut prepared_only.svm, &f.payer, &instructions).unwrap();
    let preparation_debit =
        preparation_before - prepared_only.svm.get_account(&f.payer).unwrap().lamports;
    assert!(preparation_debit < 100_000_000);

    let global = f.svm.get_account(&f.keys[14]).unwrap();
    let recipient = Address::new_from_array(global.data[41..73].try_into().unwrap());
    let buyback = Address::new_from_array(global.data[741..773].try_into().unwrap());
    // Official getBuyTokenAmountFromSolAmount on snapshot443170534 quotes
    // 66,285,714,223,523 raw. The following is its 99% minimum, not min=1.
    let minimum = 65_622_857_081_287u64;
    let mut data = vec![56, 252, 116, 8, 158, 223, 205, 95];
    data.extend_from_slice(&2_000_000_000u64.to_le_bytes());
    data.extend_from_slice(&minimum.to_le_bytes());
    data.push(0);
    let buy = public_instruction(
        PUMP,
        &[
            f.keys[14],
            recipient,
            f.keys[3],
            f.keys[16],
            f.keys[17],
            authority_base,
            f.payer,
            pinocchio_system::ID,
            TOKEN_2022,
            f.keys[27],
            f.keys[18],
            PUMP,
            derive(&[b"global_volume_accumulator"], &PUMP),
            authority_volume,
            derive(&[b"fee_config", PUMP.as_ref()], &FEES),
            FEES,
            derive(&[b"bonding-curve-v2", f.keys[3].as_ref()], &PUMP),
            buyback,
        ],
        &[1, 3, 4, 5, 6, 9, 13, 17],
        &[6],
        data,
    );
    instructions.push(buy);
    assert_eq!(
        Message::new(&instructions, Some(&f.payer))
            .header
            .num_required_signatures,
        2
    );

    let mut impossible = instructions.clone();
    impossible.last_mut().unwrap().data[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
    let rejected = f.rejects_atomically(&impossible);
    assert!(rejected.contains("Custom("), "{rejected}");
    assert!(f.svm.get_account(&authority_base).is_none());
    assert_eq!(
        f.svm.get_account(&authority_volume).unwrap().data.len(),
        137
    );
    let before = f.svm.get_account(&f.payer).unwrap().lamports;
    let units = send_many(&mut f.svm, &f.payer, &instructions).unwrap();
    let total_debit = before - f.svm.get_account(&f.payer).unwrap().lamports;
    assert_eq!(total_debit - preparation_debit, 2_000_000_000);
    let state = f.svm.get_account(&f.keys[1]).unwrap();
    assert_eq!(state.data[75], 1);
    assert_eq!(&state.data[80..112], &[0; 32]);
    validate_sharing_data(
        &f.svm.get_account(&f.keys[26]).unwrap().data,
        &f.keys[3],
        &f.keys[2],
    )
    .unwrap();
    let received = u64_at(&f.svm.get_account(&authority_base).unwrap().data, 64).unwrap();
    assert!(received >= minimum);
    assert_eq!(
        u64_at(&f.svm.get_account(&f.keys[4]).unwrap().data, 64).unwrap(),
        0
    );
    assert_eq!(
        f.svm.get_account(&f.keys[2]).unwrap().lamports,
        f.svm.minimum_balance_for_rent_exemption(0)
    );
    let creator_fees = f.svm.get_account(&f.keys[27]).unwrap().lamports
        - f.svm.minimum_balance_for_rent_exemption(0);
    assert!(creator_fees > 0);
    let net_curve = u64_at(&f.svm.get_account(&f.keys[16]).unwrap().data, 32).unwrap();
    let protocol_fees = f.svm.get_account(&recipient).unwrap().lamports
        - prepared_only.svm.get_account(&recipient).unwrap().lamports;
    let buyback_fees = f.svm.get_account(&buyback).unwrap().lamports
        - prepared_only.svm.get_account(&buyback).unwrap().lamports;
    assert_eq!(
        net_curve + protocol_fees + buyback_fees + creator_fees,
        2_000_000_000
    );
    println!("Actual production localVM atomic create+100%feeLock+finish+authoritybuy PASS: exact buydebit=2000000000; separate3volumeRent+fee={volume_preparation_debit}; atomicSetup+authorityATA+2signaturefees+priority={preparation_debit}; atomicTotalDebit={total_debit}; CU={units}; authority rawtokens={received}; curveNet={net_curve}; protocolfees={protocol_fees}; buybackfees={buyback_fees}; creatorfees={creator_fees}; no vaulttokens/burn; failed buy rolls back launch");
}

#[test]
#[ignore = "Public snapshot required; proves replacement-program boost support locally"]
fn real_mainnet_standard_graduation_buy_and_burn() {
    for use_v2 in [false, true] {
        rehearse_standard_graduation(use_v2);
    }
}

fn rehearse_standard_graduation(use_v2: bool) {
    // With default (production) identities, require the precise packaged ELF.
    // Neither the production artifact nor any upstream code is rebuilt here.
    if !cfg!(feature = "test-fixture") {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        assert_eq!(
            std::fs::read(root.join("../../target/deploy/burned_fun.so")).unwrap(),
            std::fs::read(root.join("../../../public/program/burned_fun.so")).unwrap(),
            "The local VM must execute the exact production artifact",
        );
    }
    let mut f = mainnet_fixture();
    let setup = f.setup();
    send_many(&mut f.svm, &f.payer, &setup).unwrap();
    let mint = f.keys[3];
    let treasury = f.keys[2];
    let sharing = f.keys[26];
    let curve = f.keys[16];
    let native_creator = f.keys[27];
    let amm_creator = f.keys[31];
    let amm_creator_ata = f.keys[32];
    let system = pinocchio_system::ID;
    let mint_supply = u64_at(&f.svm.get_account(&mint).unwrap().data, 36).unwrap();
    let reserve = f.svm.minimum_balance_for_rent_exemption(0);

    // These are public fixture addresses, not generated or imported signers.
    // Synthetic SOL is available exclusively inside this VM.
    let trader = Address::new_from_array([11; 32]);
    let caller = Address::new_from_array([12; 32]);
    assert_ne!(caller, f.payer);
    f.svm.airdrop(&trader, 500_000_000_000).unwrap();
    f.svm.airdrop(&caller, 50_000_000).unwrap();
    let trader_base = ata(&trader, &mint, &TOKEN_2022);
    let trader_quote = ata(&trader, &WSOL, &TOKEN);
    let trader_pump_volume = derive(&[b"user_volume_accumulator", trader.as_ref()], &PUMP);
    let trader_amm_volume = derive(&[b"user_volume_accumulator", trader.as_ref()], &AMM);
    for (account, token_mint, token) in
        [(trader_base, mint, TOKEN_2022), (trader_quote, WSOL, TOKEN)]
    {
        let ix = public_instruction(
            ASSOCIATED_TOKEN,
            &[trader, account, trader, token_mint, system, token],
            &[0, 1],
            &[0],
            vec![1],
        );
        send(&mut f.svm, &trader, ix).unwrap();
    }
    for (program, volume) in [(PUMP, trader_pump_volume), (AMM, trader_amm_volume)] {
        let ix = public_instruction(
            program,
            &[
                trader,
                trader,
                volume,
                system,
                derive(&[b"__event_authority"], &program),
                program,
            ],
            &[0, 2],
            &[0],
            vec![94, 6, 202, 115, 255, 96, 232, 183],
        );
        send(&mut f.svm, &trader, ix).unwrap();
    }

    let pump_global = f.svm.get_account(&f.keys[14]).unwrap();
    let pump_recipient = Address::new_from_array(pump_global.data[41..73].try_into().unwrap());
    let pump_buyback = Address::new_from_array(pump_global.data[741..773].try_into().unwrap());
    let withdraw = Address::new_from_array(pump_global.data[113..145].try_into().unwrap());
    let remaining_tokens = u64_at(&f.svm.get_account(&curve).unwrap().data, 24).unwrap();
    // Actual Pump buy: finish the curve by purchasing all remaining real tokens.
    // No pool, reserve, completion flag or coin-creator field is fabricated.
    let mut data = vec![102, 6, 61, 18, 1, 218, 235, 234];
    data.extend_from_slice(&remaining_tokens.to_le_bytes());
    data.extend_from_slice(&200_000_000_000u64.to_le_bytes());
    data.push(0);
    let finish_curve = public_instruction(
        PUMP,
        &[
            f.keys[14],
            pump_recipient,
            mint,
            curve,
            f.keys[17],
            trader_base,
            trader,
            system,
            TOKEN_2022,
            native_creator,
            f.keys[18],
            PUMP,
            derive(&[b"global_volume_accumulator"], &PUMP),
            trader_pump_volume,
            derive(&[b"fee_config", PUMP.as_ref()], &FEES),
            FEES,
            derive(&[b"bonding-curve-v2", mint.as_ref()], &PUMP),
            pump_buyback,
        ],
        &[1, 3, 4, 5, 6, 9, 13, 17],
        &[6],
        data,
    );
    let completion_units = send(&mut f.svm, &trader, finish_curve).unwrap();
    let completed = f.svm.get_account(&curve).unwrap();
    assert_eq!(completed.data[48], 1);
    assert_eq!(u64_at(&completed.data, 24).unwrap(), 0);
    assert_eq!(&completed.data[81..83], &[0, 0]);

    let pool_authority = derive(&[b"pool-authority", mint.as_ref()], &PUMP);
    let pool = derive(
        &[
            b"pool",
            &[0, 0],
            pool_authority.as_ref(),
            mint.as_ref(),
            WSOL.as_ref(),
        ],
        &AMM,
    );
    let pool_base = ata(&pool, &mint, &TOKEN_2022);
    let pool_quote = ata(&pool, &WSOL, &TOKEN);
    let amm_global = derive(&[b"global_config"], &AMM);
    let amm_event = derive(&[b"__event_authority"], &AMM);
    let lp_mint = derive(&[b"pool_lp_mint", pool.as_ref()], &AMM);
    let boost_authority = derive(&[b"boost_vault", pool.as_ref()], &AMM);
    // The installed SDK's migrate/migrateV2 helpers omit the two remaining
    // boost accounts now needed by upstream. Their derivations are in the
    // official AMM InitBoost IDL; finalized transaction 4F75kQwEFcmPmhh7...
    // independently demonstrates the same trailing authority/WSOL ATA pair.
    let migrate = if use_v2 {
        public_instruction(
            PUMP,
            &[
                f.keys[14],
                withdraw,
                mint,
                WSOL,
                curve,
                f.keys[17],
                ata(&curve, &WSOL, &TOKEN),
                trader,
                system,
                AMM,
                pool,
                pool_authority,
                ata(&pool_authority, &mint, &TOKEN_2022),
                ata(&pool_authority, &WSOL, &TOKEN),
                amm_global,
                lp_mint,
                ata(&pool_authority, &lp_mint, &TOKEN_2022),
                pool_base,
                pool_quote,
                TOKEN_2022,
                TOKEN,
                TOKEN_2022,
                ASSOCIATED_TOKEN,
                amm_event,
                Address::from_str_const("SysvarRent111111111111111111111111111111111"),
                f.keys[18],
                PUMP,
                boost_authority,
                ata(&boost_authority, &WSOL, &TOKEN),
            ],
            &[1, 4, 5, 6, 10, 11, 12, 13, 15, 16, 17, 18, 28],
            &[7],
            vec![187, 203, 18, 31, 206, 237, 254, 41],
        )
    } else {
        public_instruction(
            PUMP,
            &[
                f.keys[14],
                withdraw,
                mint,
                curve,
                f.keys[17],
                trader,
                system,
                TOKEN,
                AMM,
                pool,
                pool_authority,
                ata(&pool_authority, &mint, &TOKEN_2022),
                ata(&pool_authority, &WSOL, &TOKEN),
                amm_global,
                WSOL,
                lp_mint,
                ata(&pool_authority, &lp_mint, &TOKEN_2022),
                pool_base,
                pool_quote,
                TOKEN_2022,
                ASSOCIATED_TOKEN,
                amm_event,
                f.keys[18],
                PUMP,
                Address::from_str_const("SysvarRent111111111111111111111111111111111"),
                boost_authority,
                ata(&boost_authority, &WSOL, &TOKEN),
            ],
            &[1, 3, 4, 9, 10, 11, 12, 13, 15, 16, 17, 18, 26],
            &[5],
            vec![155, 234, 231, 146, 236, 158, 162, 30],
        )
    };
    let mut incomplete_migrate = migrate.clone();
    incomplete_migrate
        .accounts
        .truncate(migrate.accounts.len() - 2);
    let missing_accounts = send(&mut f.svm, &trader, incomplete_migrate).unwrap_err();
    assert!(missing_accounts.contains("Custom(6027)"));
    assert_eq!(f.svm.get_account(&curve).unwrap(), completed);
    assert!(f.svm.get_account(&pool).is_none());
    let migration_units = send(&mut f.svm, &trader, migrate).unwrap();
    let migrated = f.svm.get_account(&pool).unwrap();
    assert_eq!(migrated.owner, AMM);
    assert!(address_at(&migrated.data, 11, &pool_authority));
    assert!(address_at(&migrated.data, 43, &mint));
    assert!(address_at(&migrated.data, 75, &WSOL));
    assert!(address_at(&migrated.data, 211, &sharing));
    let virtual_quote = i128::from_le_bytes(migrated.data[245..261].try_into().unwrap());
    assert!(
        virtual_quote > 0,
        "Update the readiness regression if upstream stops auto-boosting standard pools"
    );
    assert_eq!(&migrated.data[243..245], &[0, 0]);
    println!("Real standard Token2022 graduation: migrate_v2={use_v2}; completion CU={completion_units}; migration CU={migration_units}; pool={pool}; initial bytes={}; initial rent={}; virtual_quote_reserves={virtual_quote}", migrated.data.len(), migrated.lamports);

    let global = f.svm.get_account(&amm_global).unwrap();
    let recipient = Address::new_from_array(global.data[57..89].try_into().unwrap());
    let buyback = Address::new_from_array(global.data[643..675].try_into().unwrap());
    let amm_keys = |user: Address, base: Address, wrapped: Address, volume: Address| {
        vec![
            pool,
            user,
            amm_global,
            mint,
            WSOL,
            base,
            wrapped,
            pool_base,
            pool_quote,
            recipient,
            ata(&recipient, &WSOL, &TOKEN),
            TOKEN_2022,
            TOKEN,
            system,
            ASSOCIATED_TOKEN,
            amm_event,
            AMM,
            amm_creator_ata,
            amm_creator,
            derive(&[b"global_volume_accumulator"], &AMM),
            volume,
            derive(&[b"fee_config", AMM.as_ref()], &FEES),
            FEES,
            derive(&[b"pool-v2", mint.as_ref()], &AMM),
            buyback,
            ata(&buyback, &WSOL, &TOKEN),
        ]
    };
    let mut burn_accounts: Vec<_> = [1, 2, 3, 4, 5, 26, 9, 8]
        .into_iter()
        .enumerate()
        .map(|(index, key)| AccountMeta {
            pubkey: f.keys[key],
            is_writable: index < 5,
            is_signer: false,
        })
        .collect();
    burn_accounts.extend(
        public_instruction(
            AMM,
            &amm_keys(treasury, f.keys[4], f.keys[5], f.keys[34]),
            &[0, 1, 5, 6, 7, 8, 10, 17, 20, 25],
            &[],
            vec![],
        )
        .accounts,
    );
    let mut burn_data = vec![1, 1];
    burn_data.extend_from_slice(&1u64.to_le_bytes());
    burn_data
        .extend_from_slice(&(f.svm.get_sysvar::<solana_clock::Clock>().slot + 150).to_le_bytes());
    let burn = Instruction {
        program_id: f.program,
        accounts: burn_accounts,
        data: burn_data,
    };
    let distribute = f.mapped_ix(
        PUMP,
        &[3, 16, 26, 27, 8, 18, 13, 2],
        &[3, 7],
        &[],
        vec![165, 114, 103, 0, 121, 206, 247, 81],
    );
    let initial_creator_fees = f.svm.get_account(&native_creator).unwrap().lamports - reserve;
    assert!(initial_creator_fees > 0);
    send(&mut f.svm, &caller, distribute.clone()).unwrap();
    assert_eq!(
        f.svm.get_account(&treasury).unwrap().lamports - reserve,
        initial_creator_fees
    );

    let caller_before_extension = f.svm.get_account(&caller).unwrap().lamports;
    let extend = public_instruction(
        AMM,
        &[pool, caller, system, amm_event, AMM],
        &[0],
        &[1],
        vec![234, 102, 194, 203, 150, 72, 62, 229],
    );
    let extension_units = send(&mut f.svm, &caller, extend.clone()).unwrap();
    let prepared = f.svm.get_account(&pool).unwrap();
    assert_eq!(prepared.data.len(), 301);
    let rent_topup = prepared.lamports - migrated.lamports;
    assert_eq!(
        caller_before_extension - f.svm.get_account(&caller).unwrap().lamports,
        rent_topup + 5_000
    );
    assert_eq!(
        f.svm.get_account(&treasury).unwrap().lamports - reserve,
        initial_creator_fees
    );
    assert_eq!(&prepared.data[245..261], &migrated.data[245..261]);
    println!("Real caller-paid pool extension: bytes {} -> {}; extra rent={rent_topup}; CU={extension_units}", migrated.data.len(), prepared.data.len());

    // Extending again cannot consume additional account rent.
    let caller_before = f.svm.get_account(&caller).unwrap().lamports;
    let repeated_extension = send(&mut f.svm, &caller, extend);
    assert_eq!(f.svm.get_account(&pool).unwrap(), prepared);
    assert_eq!(
        caller_before - f.svm.get_account(&caller).unwrap().lamports,
        5_000
    );
    println!(
        "Repeat pool extension: success={}; no additional rent",
        repeated_extension.is_ok()
    );

    let first_burn_units = send(&mut f.svm, &caller, burn.clone()).unwrap();
    let supply_after_curve_fees = u64_at(&f.svm.get_account(&mint).unwrap().data, 36).unwrap();
    assert!(supply_after_curve_fees < mint_supply);
    assert_eq!(
        u64_at(&f.svm.get_account(&f.keys[4]).unwrap().data, 64).unwrap(),
        0
    );
    assert!(f.svm.get_account(&treasury).unwrap().lamports >= reserve);

    // Generate AMM creator fees from an actual unrelated wallet's purchase.
    // This exceeds the upstream sweep's minimum; fees are not fabricated.
    let mut buy_data = vec![198, 46, 21, 82, 180, 217, 232, 112];
    buy_data.extend_from_slice(&10_000_000_000u64.to_le_bytes());
    buy_data.extend_from_slice(&1u64.to_le_bytes());
    buy_data.push(0);
    let ordinary_buy = public_instruction(
        AMM,
        &amm_keys(trader, trader_base, trader_quote, trader_amm_volume),
        &[0, 1, 5, 6, 7, 8, 10, 17, 20, 25],
        &[1],
        buy_data,
    );
    let fund =
        solana_system_interface::instruction::transfer(&trader, &trader_quote, 10_000_000_000);
    let sync = public_instruction(TOKEN, &[trader_quote], &[0], &[], vec![17]);
    send_many(&mut f.svm, &trader, &[fund, sync, ordinary_buy]).unwrap();
    let pending_amm = u64_at(&f.svm.get_account(&amm_creator_ata).unwrap().data, 64).unwrap();
    assert!(pending_amm >= f.svm.minimum_balance_for_rent_exemption(165));
    let sweep = public_instruction(
        AMM,
        &[
            WSOL,
            TOKEN,
            system,
            ASSOCIATED_TOKEN,
            sharing,
            amm_creator,
            amm_creator_ata,
            native_creator,
            amm_event,
            AMM,
        ],
        &[5, 6, 7],
        &[],
        vec![139, 52, 134, 85, 228, 229, 108, 241],
    );

    // Match the site's atomic sweep -> distribute -> execute bundle.
    let caller_before = f.svm.get_account(&caller).unwrap().lamports;
    let bundle_units = send_many(
        &mut f.svm,
        &caller,
        &[sweep, distribute, burn.clone()],
    )
    .unwrap();
    let state = f.svm.get_account(&f.keys[1]).unwrap();
    let treasury_after = f.svm.get_account(&treasury).unwrap().lamports;
    assert_eq!(
        caller_before - f.svm.get_account(&caller).unwrap().lamports,
        5_000
    );
    assert_eq!(
        u64_at(&f.svm.get_account(&f.keys[4]).unwrap().data, 64).unwrap(),
        0
    );
    let supply_after_bundle = u64_at(&f.svm.get_account(&mint).unwrap().data, 36).unwrap();
    assert!(supply_after_bundle < supply_after_curve_fees);
    assert!(u64_at(&state.data, 80).unwrap() > 0);
    assert_eq!(u64_at(&state.data, 88).unwrap(), mint_supply - supply_after_bundle);
    assert_eq!(u64_at(&state.data, 96).unwrap(), 2);
    assert_eq!(
        f.svm.get_account(&native_creator).unwrap().lamports,
        reserve
    );
    assert!(u64_at(&f.svm.get_account(&amm_creator_ata).unwrap().data, 64).unwrap() < pending_amm);
    assert!(treasury_after >= reserve);
    let mut extra_burns = 0;
    while f.svm.get_account(&treasury).unwrap().lamports > reserve
        || u64_at(&f.svm.get_account(&f.keys[5]).unwrap().data, 64).unwrap() > 0
    {
        assert!(extra_burns < 10, "fee vault did not drain");
        send(&mut f.svm, &caller, burn.clone()).unwrap();
        extra_burns += 1;
    }
    let final_supply = u64_at(&f.svm.get_account(&mint).unwrap().data, 36).unwrap();
    let final_state = f.svm.get_account(&f.keys[1]).unwrap();
    assert!(final_supply < supply_after_bundle);
    assert_eq!(u64_at(&final_state.data, 88).unwrap(), mint_supply - final_supply);
    assert_eq!(u64_at(&final_state.data, 96).unwrap(), 2 + extra_burns);
    assert_eq!(f.svm.get_account(&treasury).unwrap().lamports, reserve);
    assert_eq!(u64_at(&f.svm.get_account(&f.keys[5]).unwrap().data, 64).unwrap(), 0);
    validate_sharing_data(&f.svm.get_account(&sharing).unwrap().data, &mint, &treasury).unwrap();
    println!("Replacement program post-graduation PASS: migrate_v2={use_v2}; actual AMM creator fees={pending_amm}; first burn CU={first_burn_units}; atomic fee sweep+distribution+buy+burn CU={bundle_units}; follow-up burns={extra_burns}; total raw tokens burned={}; vault empty; treasury rent preserved", mint_supply - final_supply);
}
