//! Local ABI-compatible launch doubles. Real System/ATA/Token-2022 CPIs create
//! accounts and tokens; the upstream venue/fee business logic remains mocked.
use pinocchio::{
    cpi::{invoke_signed, Seed, Signer},
    error::ProgramError,
    instruction::{InstructionAccount, InstructionView},
    sysvars::{rent::Rent, Sysvar},
    AccountView, Address, ProgramResult, Resize,
};
use pinocchio_system::{create_account_with_minimum_balance_signed, instructions::Transfer};
use pinocchio_token_2022::instructions::{AuthorityType, InitializeMint2, MintTo, SetAuthority};

const PUMP: Address = Address::from_str_const("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
const FEES: Address = Address::from_str_const("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ");
const ATA: Address = Address::from_str_const("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
const TOKEN: Address = pinocchio_token_2022::ID;
const SHARING_TAG: [u8; 8] = [216, 74, 9, 0, 56, 140, 93, 75];

pub fn handle(program: &Address, a: &mut [AccountView], data: &[u8]) -> Option<ProgramResult> {
    let tag: [u8; 8] = data.get(..8)?.try_into().ok()?;
    Some(match tag {
        [214, 144, 76, 236, 95, 139, 49, 180] => create(program, a, data),
        [234, 102, 194, 203, 150, 72, 62, 229] => extend(program, a),
        [195, 78, 86, 76, 111, 52, 251, 213] => create_sharing(program, a),
        [111, 251, 49, 6, 78, 78, 106, 18] => update_sharing(program, a, data),
        [94, 6, 202, 115, 255, 96, 232, 183] => create_volume(program, a),
        [165, 114, 103, 0, 121, 206, 247, 81] => distribute(program, a),
        [87, 124, 52, 191, 52, 38, 214, 232] => {
            if program != &PUMP || a.len() != 5 || !a[2].owned_by(&FEES) {
                Err(ProgramError::InvalidArgument)
            } else {
                let address = a[2].address().clone();
                a[1].try_borrow_mut()
                    .map(|mut curve| curve[49..81].copy_from_slice(address.as_ref()))
            }
        }
        _ => return None,
    })
}

fn distribute(program: &Address, a: &mut [AccountView]) -> ProgramResult {
    if program != &PUMP || a.len() != 8 || !a[2].owned_by(&FEES) {
        return Err(ProgramError::InvalidArgument);
    }
    if a[2].try_borrow()?.get(80..112) != Some(a[7].address().as_ref()) {
        return Err(ProgramError::InvalidArgument);
    }
    let amount = a[3]
        .lamports()
        .saturating_sub(Rent::get()?.try_minimum_balance(0)?);
    if amount != 0 {
        let (_, bump) =
            Address::find_program_address(&[b"creator-vault", a[2].address().as_ref()], program);
        let bump = [bump];
        let seeds = [
            Seed::from(b"creator-vault".as_slice()),
            Seed::from(a[2].address().as_ref()),
            Seed::from(&bump),
        ];
        Transfer {
            from: &a[3],
            to: &a[7],
            lamports: amount,
        }
        .invoke_signed(&[Signer::from(&seeds)])?;
    }
    Ok(())
}

fn extend(program: &Address, a: &mut [AccountView]) -> ProgramResult {
    if program != &PUMP || a.len() != 5 || !a[0].owned_by(program) || !a[1].is_signer() {
        return Err(ProgramError::InvalidArgument);
    }
    let topup = Rent::get()?
        .try_minimum_balance(151)?
        .saturating_sub(a[0].lamports());
    if topup > 0 {
        Transfer {
            from: &a[1],
            to: &a[0],
            lamports: topup,
        }
        .invoke()?;
    }
    if a[0].data_len() < 151 {
        a[0].resize(151)?;
    }
    Ok(())
}

fn create(program: &Address, a: &mut [AccountView], data: &[u8]) -> ProgramResult {
    if program != &PUMP
        || a.len() != 16
        || data.len() < 46
        || data[data.len() - 2..] != [0, 0]
        || data[data.len() - 34..data.len() - 2] != *a[5].address().as_ref()
    {
        return Err(ProgramError::InvalidInstructionData);
    }
    let mint_address = a[0].address().clone();
    let payer = a[5].clone();
    create_account_with_minimum_balance_signed(&mut a[0], 82, &TOKEN, &payer, None, &[])?;
    InitializeMint2 {
        mint: &a[0],
        decimals: 6,
        mint_authority: a[1].address(),
        freeze_authority: None,
        token_program: &TOKEN,
    }
    .invoke()?;
    let (_, curve_bump) =
        Address::find_program_address(&[b"bonding-curve", mint_address.as_ref()], program);
    let curve_bump = [curve_bump];
    let curve_seeds = [
        Seed::from(b"bonding-curve".as_slice()),
        Seed::from(mint_address.as_ref()),
        Seed::from(&curve_bump),
    ];
    create_account_with_minimum_balance_signed(
        &mut a[2],
        115,
        program,
        &payer,
        None,
        &[Signer::from(&curve_seeds)],
    )?;
    let ata_accounts = [&a[5], &a[3], &a[2], &a[0], &a[6], &a[7]];
    let ata_metas: [InstructionAccount; 6] =
        core::array::from_fn(|i| InstructionAccount::new(ata_accounts[i].address(), i < 2, i == 0));
    invoke_signed(
        &InstructionView {
            program_id: &ATA,
            accounts: &ata_metas,
            data: &[1],
        },
        &ata_accounts,
        &[],
    )?;
    let (_, mint_bump) = Address::find_program_address(&[b"mint-authority"], program);
    let mint_bump = [mint_bump];
    let mint_seeds = [
        Seed::from(b"mint-authority".as_slice()),
        Seed::from(&mint_bump),
    ];
    let signer = [Signer::from(&mint_seeds)];
    MintTo {
        mint: &a[0],
        account: &a[3],
        mint_authority: &a[1],
        amount: 1_000_000_000_000,
        token_program: &TOKEN,
    }
    .invoke_signed(&signer)?;
    SetAuthority {
        account: &a[0],
        authority: &a[1],
        authority_type: AuthorityType::MintTokens,
        new_authority: None,
        token_program: &TOKEN,
    }
    .invoke_signed(&signer)?;
    let mut curve = a[2].try_borrow_mut()?;
    curve[..8].copy_from_slice(&[23, 183, 248, 55, 96, 216, 172, 96]);
    for (offset, value) in [
        (8, 1_000_000_000_000u64),
        (16, 30_000_000_000),
        (24, 793_100_000_000),
        (40, 1_000_000_000_000),
    ] {
        curve[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    curve[49..81].copy_from_slice(payer.address().as_ref());
    Ok(())
}

fn create_sharing(program: &Address, a: &mut [AccountView]) -> ProgramResult {
    if program != &FEES || a.len() != 13 || a[10].address() != &FEES {
        return Err(ProgramError::InvalidArgument);
    }
    let payer = a[2].clone();
    let mint = a[4].address().clone();
    let (_, bump) = Address::find_program_address(&[b"sharing-config", mint.as_ref()], program);
    let bump = [bump];
    let seeds = [
        Seed::from(b"sharing-config".as_slice()),
        Seed::from(mint.as_ref()),
        Seed::from(&bump),
    ];
    create_account_with_minimum_balance_signed(
        &mut a[5],
        114,
        program,
        &payer,
        None,
        &[Signer::from(&seeds)],
    )?;
    {
        let mut sharing = a[5].try_borrow_mut()?;
        sharing[..8].copy_from_slice(&SHARING_TAG);
        sharing[9] = 2;
        sharing[10] = 1;
        sharing[11..43].copy_from_slice(mint.as_ref());
        sharing[43..75].copy_from_slice(payer.address().as_ref());
        sharing[76..80].copy_from_slice(&1u32.to_le_bytes());
        sharing[80..112].copy_from_slice(payer.address().as_ref());
        sharing[112..114].copy_from_slice(&10_000u16.to_le_bytes());
    }
    let accounts = [&a[4], &a[7], &a[5], &a[9], &a[8]];
    let metas: [InstructionAccount; 5] =
        core::array::from_fn(|i| InstructionAccount::new(accounts[i].address(), i == 1, false));
    invoke_signed(
        &InstructionView {
            program_id: &PUMP,
            accounts: &metas,
            data: &[87, 124, 52, 191, 52, 38, 214, 232],
        },
        &accounts,
        &[],
    )
}

fn update_sharing(program: &Address, a: &mut [AccountView], data: &[u8]) -> ProgramResult {
    if program != &FEES
        || a.len() != 20
        || data.len() != 46
        || data[8..12] != [1, 0, 0, 0]
        || a[2].address() != a[19].address()
    {
        return Err(ProgramError::InvalidInstructionData);
    }
    let mode = a[3].try_borrow()?.get(999).copied().unwrap_or(0);
    if mode == 3 {
        return Err(ProgramError::Custom(999));
    }
    // Like the real native-SOL fee update, settle the initial creator's share
    // before installing the permanent recipient. This includes donated SOL.
    let accounts = [&a[4], &a[6], &a[5], &a[7], &a[9], &a[11], &a[10], &a[19]];
    let metas: [InstructionAccount; 8] = core::array::from_fn(|i| {
        InstructionAccount::new(accounts[i].address(), i == 3 || i == 7, false)
    });
    invoke_signed(
        &InstructionView {
            program_id: &PUMP,
            accounts: &metas,
            data: &[165, 114, 103, 0, 121, 206, 247, 81],
        },
        &accounts,
        &[],
    )?;
    let payer = a[2].address().clone();
    let mut sharing = a[5].try_borrow_mut()?;
    sharing[75] = u8::from(mode != 2);
    sharing[80..112].copy_from_slice(if mode == 1 {
        payer.as_ref()
    } else {
        &data[12..44]
    });
    sharing[112..114].copy_from_slice(&data[44..46]);
    Ok(())
}

fn create_volume(program: &Address, a: &mut [AccountView]) -> ProgramResult {
    if a.len() != 6 {
        return Err(ProgramError::InvalidArgument);
    }
    let payer = a[0].clone();
    let user = a[1].address().clone();
    let (_, bump) =
        Address::find_program_address(&[b"user_volume_accumulator", user.as_ref()], program);
    let bump = [bump];
    let seeds = [
        Seed::from(b"user_volume_accumulator".as_slice()),
        Seed::from(user.as_ref()),
        Seed::from(&bump),
    ];
    create_account_with_minimum_balance_signed(
        &mut a[2],
        if program == &PUMP { 106 } else { 90 },
        program,
        &payer,
        None,
        &[Signer::from(&seeds)],
    )?;
    let mut volume = a[2].try_borrow_mut()?;
    volume[..8].copy_from_slice(&[86, 255, 112, 14, 102, 53, 154, 250]);
    volume[8..40].copy_from_slice(user.as_ref());
    Ok(())
}
