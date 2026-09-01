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

fn mint_data(supply: u64) -> Vec<u8> {
    let mut data = vec![0; 82];
    data[36..44].copy_from_slice(&supply.to_le_bytes());
    data[44] = 6;
    data[45] = 1;
    data
}

fn sharing_data(mint: &Address, treasury: &Address) -> Vec<u8> {
    let mut data = vec![0; 114];
    data[..8].copy_from_slice(&SHARING_TAG);
    data[9] = 2;
    data[10] = 1;
    data[11..43].copy_from_slice(mint.as_ref());
    data[75] = 1;
    data[76..80].copy_from_slice(&1u32.to_le_bytes());
    data[80..112].copy_from_slice(treasury.as_ref());
    data[112..114].copy_from_slice(&10_000u16.to_le_bytes());
    data
}

#[test]
fn trust_boundaries_and_budget_math_fail_closed() {
    let mint = Address::new_unique();
    let treasury = Address::new_unique();
    let valid = sharing_data(&mint, &treasury);
    assert!(validate_sharing_data(&valid, &mint, &treasury).is_ok());
    for offset in [0, 9, 10, 11, 75, 76, 80, 112] {
        let mut bad = valid.clone();
        bad[offset] ^= 1;
        assert!(
            validate_sharing_data(&bad, &mint, &treasury).is_err(),
            "sharing offset {offset}"
        );
    }
    assert!(validate_sharing_data(&valid[..113], &mint, &treasury).is_err());
    let valid_mint = mint_data(1_000_000);
    assert_eq!(validate_mint_data(&valid_mint).unwrap(), 6);
    for offset in [0, 45, 46] {
        let mut bad = valid_mint.clone();
        bad[offset] ^= 1;
        assert!(validate_mint_data(&bad).is_err());
    }
    let mut extended = valid_mint.clone();
    extended.resize(166, 0);
    extended[165] = 1;
    extended.extend_from_slice(&[18, 0, 64, 0]);
    extended.resize(234, 0);
    assert!(validate_mint_data(&extended).is_ok());
    for extension in [1, 3, 6, 12, 14, 16, 26] {
        let mut bad = extended.clone();
        bad[166] = extension;
        assert!(validate_mint_data(&bad).is_err(), "extension {extension}");
    }
    let mut hidden_extension = extended;
    hidden_extension.extend_from_slice(&[0, 0, 0, 0, 14, 0, 32, 0]);
    hidden_extension.resize(274, 0);
    assert!(validate_mint_data(&hidden_extension).is_err());
    assert_eq!(effective_quote_reserve(2_000, &0i128.to_le_bytes()).unwrap(), 2_000);
    assert_eq!(effective_quote_reserve(2_000, &3_000i128.to_le_bytes()).unwrap(), 5_000);
    assert_eq!(effective_quote_reserve(2_000, &(-1_000i128).to_le_bytes()).unwrap(), 1_000);
    assert!(effective_quote_reserve(2_000, &(-2_000i128).to_le_bytes()).is_err());
    assert!(effective_quote_reserve(2_000, &[0; 15]).is_err());
    assert_eq!(
        trade_limits(1_000_000_000, 1_000_000_000, 10_000_000_000, u64::MAX).unwrap(),
        (100_000_000, 9_405_940)
    );
    assert_eq!(
        trade_limits(1_000_000_000, u64::MAX, 100_000_000, u64::MAX)
            .unwrap()
            .0,
        1_000_000
    );
    assert_eq!(
        trade_limits(100_000_000, u64::MAX, u64::MAX.into(), 100)
            .unwrap()
            .1,
        95
    );
    assert!(trade_limits(0, 1, 1, 1).is_err());
    assert!(trade_limits(1, 1, u64::MAX.into(), 1).is_err());
    assert!(validate_deadline(100, 100).is_ok());
    assert!(validate_deadline(100, 250).is_ok());
    assert!(validate_deadline(100, 99).is_err());
    assert!(validate_deadline(100, 251).is_err());
}

// Only public test addresses are used; no private keys are created or read.
// Signature checks are disabled only in this local VM. Pump/AMM are mocked;
// System, SPL Token, Token-2022 and BurnChecked are real bundled VM programs.
struct Fixture {
    svm: LiteSVM,
    program: Address,
    payer: Address,
    anyone: Address,
    mint: Address,
    config: Address,
    treasury: Address,
    base: Address,
    wrapped: Address,
    sharing: Address,
    curve: Address,
    token_program: Address,
    pump_accounts: Vec<AccountMeta>,
    amm_accounts: Vec<AccountMeta>,
}

fn put(
    svm: &mut LiteSVM,
    template: &Address,
    address: &Address,
    owner: &Address,
    data: Vec<u8>,
    extra_lamports: u64,
) {
    let mut account = svm.get_account(template).unwrap();
    account.lamports = svm.minimum_balance_for_rent_exemption(data.len()) + extra_lamports;
    account.owner = owner.clone();
    account.data = data;
    account.executable = false;
    svm.set_account(address.clone(), account).unwrap();
}

fn put_ata(
    svm: &mut LiteSVM,
    template: &Address,
    mint: &Address,
    owner: &Address,
    program: &Address,
    amount: u64,
) -> Address {
    let address = pda(
        &[owner.as_ref(), program.as_ref(), mint.as_ref()],
        &ASSOCIATED_TOKEN,
    )
    .unwrap()
    .0;
    let mut data = vec![0; 165];
    data[..32].copy_from_slice(mint.as_ref());
    data[32..64].copy_from_slice(owner.as_ref());
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    data[108] = 1;
    if mint == &WSOL {
        data[109..113].copy_from_slice(&1u32.to_le_bytes());
        data[113..121].copy_from_slice(&svm.minimum_balance_for_rent_exemption(165).to_le_bytes());
    }
    if program == &TOKEN_2022 {
        data.extend_from_slice(&[2, 7, 0, 0, 0]); // Account type + ImmutableOwner.
    }
    put(
        svm,
        template,
        &address,
        program,
        data,
        if mint == &WSOL { amount } else { 0 },
    );
    address
}

fn metas(addresses: &[Address], writable: &[usize]) -> Vec<AccountMeta> {
    addresses
        .iter()
        .enumerate()
        .map(|(i, key)| AccountMeta {
            pubkey: key.clone(),
            is_signer: false,
            is_writable: writable.contains(&i),
        })
        .collect()
}

fn send(svm: &mut LiteSVM, payer: &Address, instruction: Instruction) -> Result<(), String> {
    send_many(svm, payer, &[instruction])
}

fn send_many(
    svm: &mut LiteSVM,
    payer: &Address,
    instructions: &[Instruction],
) -> Result<(), String> {
    svm.expire_blockhash();
    let mut limit = vec![2]; // ComputeBudget SetComputeUnitLimit.
    limit.extend_from_slice(&1_000_000u32.to_le_bytes());
    let budget = Instruction {
        program_id: Address::from_str_const("ComputeBudget111111111111111111111111111111"),
        accounts: vec![],
        data: limit,
    };
    let mut all = vec![budget];
    all.extend_from_slice(instructions);
    let mut tx = Transaction::new_unsigned(Message::new(&all, Some(payer)));
    tx.message.recent_blockhash = svm.latest_blockhash();
    svm.send_transaction(tx)
        .map(|_| ())
        .map_err(|e| format!("{e:#?}"))
}

fn begin_instruction(program: &Address, payer: &Address, mint: &Address) -> Instruction {
    let mut accounts = metas(
        &[
            payer.clone(),
            pda(&[b"buyback", mint.as_ref()], program).unwrap().0,
            pda(&[b"treasury", mint.as_ref()], program).unwrap().0,
            mint.clone(),
            pinocchio_system::ID,
            program.clone(),
            pda(&[program.as_ref()], &UPGRADEABLE_LOADER).unwrap().0,
        ],
        &[0, 1, 2],
    );
    accounts[0].is_signer = true;
    Instruction {
        program_id: program.clone(),
        accounts,
        data: vec![0],
    }
}

impl Fixture {
    fn new() -> Self {
        let mut fixture = Self::prepare(TOKEN_2022);
        let finish = fixture.finish_ix();
        send(&mut fixture.svm, &fixture.payer, finish).unwrap();
        fixture
    }

    fn prepare(token_program: Address) -> Self {
        let mut svm = LiteSVM::new().with_mainnet_features().with_sigverify(false);
        let payer = identity::AUTHORITY;
        let anyone = Address::new_unique();
        svm.airdrop(&payer, 10_000_000_000).unwrap();
        svm.airdrop(&anyone, 10_000_000_000).unwrap();
        let program = Address::new_unique();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy");
        svm.add_program_from_file(program.clone(), root.join("burned_fun.so"))
            .expect("run sh contract/build.sh first");
        for venue in [&PUMP, &AMM, &FEES] {
            svm.add_program_from_file(venue.clone(), root.join("mock_venue.so"))
                .unwrap();
        }
        let mint = identity::MINT;
        let config = pda(&[b"buyback", mint.as_ref()], &program).unwrap().0;
        let treasury = pda(&[b"treasury", mint.as_ref()], &program).unwrap().0;
        let sharing = pda(&[b"sharing-config", mint.as_ref()], &FEES).unwrap().0;
        // Begin must precede mint creation. The launch tests exercise the full
        // atomic SDK batch; these burn fixtures inject venue state locally.
        let begin = begin_instruction(&program, &payer, &mint);
        send(&mut svm, &payer, begin).unwrap();
        let mut base_mint_data = mint_data(1_000_000_000_000);
        if token_program == TOKEN_2022 {
            base_mint_data.resize(166, 0);
            base_mint_data[165] = 1;
            base_mint_data.extend_from_slice(&[18, 0, 64, 0]); // MetadataPointer.
            base_mint_data.resize(234, 0);
        }
        put(&mut svm, &payer, &mint, &token_program, base_mint_data, 0);
        let mut wsol_mint = mint_data(0);
        wsol_mint[44] = 9;
        put(&mut svm, &payer, &WSOL, &TOKEN, wsol_mint, 0);
        put(
            &mut svm,
            &payer,
            &treasury,
            &pinocchio_system::ID,
            vec![],
            500_000_000,
        );
        let base = put_ata(&mut svm, &payer, &mint, &treasury, &token_program, 25);
        let wrapped = put_ata(&mut svm, &payer, &WSOL, &treasury, &TOKEN, 0);
        put(
            &mut svm,
            &payer,
            &sharing,
            &FEES,
            sharing_data(&mint, &treasury),
            0,
        );
        let recipient = Address::new_unique();
        let buyback = Address::new_unique();
        let curve = pda(&[b"bonding-curve", mint.as_ref()], &PUMP).unwrap().0;
        let curve_base = put_ata(
            &mut svm,
            &payer,
            &mint,
            &curve,
            &token_program,
            500_000_000_000,
        );
        let mut curve_data = vec![0; 151];
        curve_data[..8].copy_from_slice(&[23, 183, 248, 55, 96, 216, 172, 96]);
        for (offset, n) in [
            (8, 1_000_000_000_000u64),
            (16, 30_000_000_000),
            (24, 500_000_000_000),
        ] {
            curve_data[offset..offset + 8].copy_from_slice(&n.to_le_bytes());
        }
        curve_data[49..81].copy_from_slice(sharing.as_ref());
        if token_program == TOKEN_2022 {
            curve_data[83..115].copy_from_slice(WSOL.as_ref());
        }
        put(&mut svm, &payer, &curve, &PUMP, curve_data, 30_000_000_000);
        let pump_global = pda(&[b"global"], &PUMP).unwrap().0;
        let mut global_data = vec![0; 997];
        global_data[..8].copy_from_slice(&[167, 232, 232, 177, 200, 108, 114, 127]);
        global_data[41..73].copy_from_slice(recipient.as_ref());
        global_data[741..773].copy_from_slice(buyback.as_ref());
        put(&mut svm, &payer, &pump_global, &PUMP, global_data, 0);
        for venue in [&PUMP, &AMM] {
            let global_volume = pda(&[b"global_volume_accumulator"], venue).unwrap().0;
            put(&mut svm, &payer, &global_volume, venue, vec![0; 8], 0);
            let user_volume = pda(&[b"user_volume_accumulator", treasury.as_ref()], venue)
                .unwrap()
                .0;
            let mut volume_data = vec![0; if venue == &PUMP { 106 } else { 90 }];
            volume_data[..8].copy_from_slice(&VOLUME_TAG);
            volume_data[8..40].copy_from_slice(treasury.as_ref());
            put(&mut svm, &payer, &user_volume, venue, volume_data, 0);
            let fee_config = pda(&[b"fee_config", venue.as_ref()], &FEES).unwrap().0;
            put(&mut svm, &payer, &fee_config, &FEES, vec![0; 8], 0);
        }
        let creator_vault = pda(&[b"creator-vault", sharing.as_ref()], &PUMP).unwrap().0;
        put(
            &mut svm,
            &payer,
            &creator_vault,
            &pinocchio_system::ID,
            vec![],
            0,
        );
        let pump_accounts = metas(
            &[
                pump_global,
                recipient.clone(),
                mint.clone(),
                curve.clone(),
                curve_base,
                base.clone(),
                treasury.clone(),
                pinocchio_system::ID,
                token_program.clone(),
                creator_vault,
                pda(&[b"__event_authority"], &PUMP).unwrap().0,
                PUMP,
                pda(&[b"global_volume_accumulator"], &PUMP).unwrap().0,
                pda(&[b"user_volume_accumulator", treasury.as_ref()], &PUMP)
                    .unwrap()
                    .0,
                pda(&[b"fee_config", PUMP.as_ref()], &FEES).unwrap().0,
                FEES,
                pda(&[b"bonding-curve-v2", mint.as_ref()], &PUMP).unwrap().0,
                buyback.clone(),
            ],
            &[1, 3, 4, 5, 6, 9, 13, 17],
        );
        let pool_authority = pda(&[b"pool-authority", mint.as_ref()], &PUMP).unwrap().0;
        let pool = pda(
            &[
                b"pool",
                &[0, 0],
                pool_authority.as_ref(),
                mint.as_ref(),
                WSOL.as_ref(),
            ],
            &AMM,
        )
        .unwrap()
        .0;
        let pool_base = put_ata(
            &mut svm,
            &payer,
            &mint,
            &pool,
            &token_program,
            400_000_000_000,
        );
        let pool_quote = put_ata(&mut svm, &payer, &WSOL, &pool, &TOKEN, 2_000_000_000);
        let recipient_quote = put_ata(&mut svm, &payer, &WSOL, &recipient, &TOKEN, 0);
        let buyback_quote = put_ata(&mut svm, &payer, &WSOL, &buyback, &TOKEN, 0);
        let creator_vault = pda(&[b"creator_vault", sharing.as_ref()], &AMM).unwrap().0;
        let creator_quote = put_ata(&mut svm, &payer, &WSOL, &creator_vault, &TOKEN, 0);
        let mut pool_data = vec![0; 261];
        pool_data[..8].copy_from_slice(&[241, 154, 109, 4, 17, 177, 109, 188]);
        for (offset, key) in [
            (11, &pool_authority),
            (43, &mint),
            (75, &WSOL),
            (139, &pool_base),
            (171, &pool_quote),
            (211, &sharing),
        ] {
            pool_data[offset..offset + 32].copy_from_slice(key.as_ref());
        }
        put(&mut svm, &payer, &pool, &AMM, pool_data, 0);
        let amm_global = pda(&[b"global_config"], &AMM).unwrap().0;
        let mut global_data = vec![0; 899];
        global_data[..8].copy_from_slice(&[149, 8, 156, 202, 160, 252, 176, 217]);
        global_data[57..89].copy_from_slice(recipient.as_ref());
        global_data[643..675].copy_from_slice(buyback.as_ref());
        put(&mut svm, &payer, &amm_global, &AMM, global_data, 0);
        let amm_accounts = metas(
            &[
                pool,
                treasury.clone(),
                amm_global,
                mint.clone(),
                WSOL,
                base.clone(),
                wrapped.clone(),
                pool_base,
                pool_quote,
                recipient,
                recipient_quote,
                token_program.clone(),
                TOKEN,
                pinocchio_system::ID,
                ASSOCIATED_TOKEN,
                pda(&[b"__event_authority"], &AMM).unwrap().0,
                AMM,
                creator_quote,
                creator_vault,
                pda(&[b"global_volume_accumulator"], &AMM).unwrap().0,
                pda(&[b"user_volume_accumulator", treasury.as_ref()], &AMM)
                    .unwrap()
                    .0,
                pda(&[b"fee_config", AMM.as_ref()], &FEES).unwrap().0,
                FEES,
                pda(&[b"pool-v2", mint.as_ref()], &AMM).unwrap().0,
                buyback,
                buyback_quote,
            ],
            &[0, 1, 5, 6, 7, 8, 10, 17, 20, 25],
        );
        Self {
            svm,
            program,
            payer,
            anyone,
            mint,
            config,
            treasury,
            base,
            wrapped,
            sharing,
            curve,
            token_program,
            pump_accounts,
            amm_accounts,
        }
    }

    fn begin_ix(&self) -> Instruction {
        begin_instruction(&self.program, &self.payer, &self.mint)
    }

    fn finish_ix(&self) -> Instruction {
        let mut accounts = metas(
            &[
                self.payer.clone(),
                self.config.clone(),
                self.treasury.clone(),
                self.mint.clone(),
                self.base.clone(),
                self.wrapped.clone(),
                self.token_program.clone(),
                pinocchio_system::ID,
                self.sharing.clone(),
                self.program.clone(),
                pda(&[self.program.as_ref()], &UPGRADEABLE_LOADER)
                    .unwrap()
                    .0,
                self.curve.clone(),
                self.pump_accounts[9].pubkey.clone(),
                self.pump_accounts[13].pubkey.clone(),
                self.amm_accounts[20].pubkey.clone(),
            ],
            &[0, 1, 2],
        );
        accounts[0].is_signer = true;
        Instruction {
            program_id: self.program.clone(),
            accounts,
            data: vec![2],
        }
    }

    fn execute_ix(&self, venue: u8) -> Instruction {
        let mut accounts = metas(
            &[
                self.config.clone(),
                self.treasury.clone(),
                self.mint.clone(),
                self.base.clone(),
                self.wrapped.clone(),
                self.sharing.clone(),
                self.token_program.clone(),
                pinocchio_system::ID,
            ],
            &[0, 1, 2, 3, 4],
        );
        accounts.extend_from_slice(if venue == 0 {
            &self.pump_accounts
        } else {
            &self.amm_accounts
        });
        let mut data = vec![1, venue];
        data.extend_from_slice(&1u64.to_le_bytes());
        data.extend_from_slice(
            &(self.svm.get_sysvar::<solana_clock::Clock>().slot + 100).to_le_bytes(),
        );
        Instruction {
            program_id: self.program.clone(),
            accounts,
            data,
        }
    }

    fn number(&self, account: &Address, offset: usize) -> u64 {
        u64_at(&self.svm.get_account(account).unwrap().data, offset).unwrap()
    }
    fn native(&self) -> u64 {
        self.svm.get_account(&self.treasury).unwrap().lamports
    }
    fn mutate(&mut self, address: &Address, offset: usize, byte: u8) {
        let mut account = self.svm.get_account(address).unwrap();
        account.data[offset] = byte;
        self.svm.set_account(address.clone(), account).unwrap();
    }

    fn assert_failure_unchanged(&mut self, ix: Instruction) {
        let addresses = [
            &self.config,
            &self.treasury,
            &self.base,
            &self.wrapped,
            &self.mint,
            &self.curve,
        ];
        let before: Vec<_> = addresses
            .iter()
            .map(|address| self.svm.get_account(address).unwrap())
            .collect();
        assert!(send(&mut self.svm, &self.anyone, ix).is_err());
        for (address, previous) in addresses.iter().zip(before) {
            assert_eq!(
                self.svm.get_account(address).unwrap(),
                previous,
                "rollback {address}"
            );
        }
    }
}

#[test]
fn only_token_2022_coins_activate_and_any_wallet_can_buy_and_burn() {
    let mut legacy = Fixture::prepare(TOKEN);
    let treasury_before = legacy.native();
    let pending = legacy.svm.get_account(&legacy.config).unwrap();
    let finish = legacy.finish_ix();
    assert!(send(&mut legacy.svm, &legacy.payer, finish).is_err());
    assert_eq!(legacy.svm.get_account(&legacy.config).unwrap(), pending);
    assert_eq!(pending.data[75], 0);
    assert_eq!(legacy.native(), treasury_before);

    let mut f = Fixture::new();
    let supply = f.number(&f.mint, 36);
    let before = f.native();
    let ix = f.execute_ix(0);
    send(&mut f.svm, &f.anyone, ix).unwrap();
    assert_eq!(f.native(), before - MAX_SPEND);
    assert_eq!(f.number(&f.config, 80), MAX_SPEND);
    assert_eq!(f.number(&f.base, 64), 0);
    assert_eq!(f.number(&f.config, 96), 1);
    let burned = f.number(&f.config, 88);
    assert!(burned > 25); // Includes pre-existing donated tokens.
    assert_eq!(f.number(&f.mint, 36), supply - burned);
    let init_again = f.begin_ix();
    f.assert_failure_unchanged(init_again);
}

#[test]
fn amm_consumes_wrapped_fees_then_wraps_native_surplus_and_burns_every_token() {
    let mut f = Fixture::new();
    put_ata(&mut f.svm, &f.payer, &WSOL, &f.treasury, &TOKEN, 30_000_000);
    let native = f.native();
    let ix = f.execute_ix(1);
    send(&mut f.svm, &f.anyone, ix).unwrap();
    assert_eq!(f.native(), native);
    assert_eq!(f.number(&f.wrapped, 64), 10_000_000);
    assert_eq!(f.number(&f.config, 80), 20_000_000);
    assert_eq!(f.number(&f.base, 64), 0);
    let ix = f.execute_ix(1);
    send(&mut f.svm, &f.payer, ix).unwrap();
    assert!(f.native() < native);
    assert_eq!(f.number(&f.wrapped, 64), 0);
    assert_eq!(f.number(&f.base, 64), 0);
    assert_eq!(f.number(&f.config, 96), 2);
    assert_eq!(
        f.number(&f.mint, 36),
        1_000_000_000_000 - f.number(&f.config, 88)
    );
}

#[test]
fn slippage_and_overspend_roll_back_the_entire_swap() {
    let mut f = Fixture::new();
    for mode in [1, 2, 3, 4, 5] {
        f.mutate(&f.curve.clone(), 115, mode);
        let ix = f.execute_ix(0);
        f.assert_failure_unchanged(ix);
    }
}

#[test]
fn a_failure_in_the_real_token_burn_rolls_back_the_purchase() {
    let mut f = Fixture::new();
    // Corrupt only the local test supply so the real token program cannot burn
    // the purchased amount. This state is never submitted to a network.
    let mut mint = f.svm.get_account(&f.mint).unwrap();
    mint.data[36..44].fill(0);
    f.svm.set_account(f.mint.clone(), mint).unwrap();
    let ix = f.execute_ix(0);
    f.assert_failure_unchanged(ix);
}

#[test]
fn fee_route_vault_and_venue_substitution_are_rejected() {
    let mut f = Fixture::new();
    let creator_vault = f.pump_accounts[9].pubkey.clone();
    let prepared = f.svm.get_account(&creator_vault).unwrap();
    let mut underfunded = prepared.clone();
    underfunded.lamports -= 1;
    f.svm
        .set_account(creator_vault.clone(), underfunded)
        .unwrap();
    let ix = f.execute_ix(0);
    f.assert_failure_unchanged(ix);
    f.svm.set_account(creator_vault, prepared).unwrap();
    for offset in [75, 80, 112] {
        let original = f.svm.get_account(&f.sharing).unwrap();
        f.mutate(&f.sharing.clone(), offset, original.data[offset] ^ 1);
        let ix = f.execute_ix(0);
        f.assert_failure_unchanged(ix);
        f.svm.set_account(f.sharing.clone(), original).unwrap();
    }
    for index in [2, 3, 5, 8 + 1, 8 + 3, 8 + 5, 8 + 6, 8 + 11, 8 + 17] {
        let mut ix = f.execute_ix(0);
        ix.accounts[index].pubkey = f.anyone.clone();
        f.assert_failure_unchanged(ix);
    }
    for index in [8, 8 + 4, 8 + 6, 8 + 9, 8 + 17, 8 + 24, 8 + 25] {
        let mut ix = f.execute_ix(1);
        ix.accounts[index].pubkey = f.anyone.clone();
        f.assert_failure_unchanged(ix);
    }
    for data in [vec![2], vec![3], vec![1, 0], vec![255]] {
        let mut ix = f.execute_ix(0);
        ix.data = data;
        f.assert_failure_unchanged(ix);
    }
    let mut expired = f.execute_ix(0);
    expired.data[10..18].copy_from_slice(&u64::MAX.to_le_bytes());
    f.assert_failure_unchanged(expired);
}

#[test]
fn pending_state_cannot_spend_and_only_setup_authority_can_activate() {
    let mut f = Fixture::prepare(TOKEN_2022);
    // Zero decimals match the pending state's byte74. Rejection must depend
    // on pending byte75, not accidentally on the yet-unfilled decimals byte.
    let mut mint = f.svm.get_account(&f.mint).unwrap();
    mint.data[44] = 0;
    f.svm.set_account(f.mint.clone(), mint).unwrap();
    assert_eq!(f.svm.get_account(&f.config).unwrap().data[75], 0);
    assert_eq!(f.number(&f.config, 104), 10_000_000_000 - 5_000);
    let buy = f.execute_ix(0);
    f.assert_failure_unchanged(buy);
    let mut unauthorized = f.finish_ix();
    unauthorized.accounts[0] = AccountMeta::new(f.anyone.clone(), true);
    f.assert_failure_unchanged(unauthorized);
    let finish = f.finish_ix();
    send(&mut f.svm, &f.payer, finish).unwrap();
    assert_eq!(f.svm.get_account(&f.config).unwrap().data[75], 1);
    assert_eq!(f.number(&f.config, 104), 0);
    let buy = f.execute_ix(0);
    send(&mut f.svm, &f.anyone, buy).unwrap();
    assert_eq!(f.number(&f.config, 96), 1);
}
