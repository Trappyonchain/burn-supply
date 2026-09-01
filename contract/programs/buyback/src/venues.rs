use crate::validation::*;
use pinocchio::{
    cpi::{invoke_signed_with_bounds, Signer},
    error::ProgramError,
    instruction::{InstructionAccount, InstructionView},
    AccountView, Address, ProgramResult,
};

const POOL_TAG: [u8; 8] = [241, 154, 109, 4, 17, 177, 109, 188];
const PUMP_GLOBAL_TAG: [u8; 8] = [167, 232, 232, 177, 200, 108, 114, 127];
const AMM_GLOBAL_TAG: [u8; 8] = [149, 8, 156, 202, 160, 252, 176, 217];

#[allow(clippy::too_many_arguments)]
pub fn validate(
    venue: u8,
    a: &[AccountView],
    mint: &AccountView,
    treasury: &AccountView,
    base: &AccountView,
    wrapped: &AccountView,
    sharing: &AccountView,
    token_program: &AccountView,
) -> Result<(u64, u128, u64), ProgramError> {
    if venue == 0 {
        validate_curve(a, mint, treasury, base, sharing, token_program)
    } else {
        validate_pool(a, mint, treasury, base, wrapped, sharing, token_program)
    }
}

fn common(
    a: &[AccountView],
    venue: &Address,
    global: usize,
    event: usize,
    program: usize,
    global_volume: usize,
    user_volume: usize,
    fee_config: usize,
    fee_program: usize,
    treasury: &Address,
) -> ProgramResult {
    require_program(&a[program], venue)?;
    require_program(&a[fee_program], &FEES)?;
    require_pda(
        &a[global],
        &[if venue == &PUMP {
            b"global".as_slice()
        } else {
            b"global_config".as_slice()
        }],
        venue,
    )?;
    require_pda(&a[event], &[b"__event_authority"], venue)?;
    require_pda(&a[global_volume], &[b"global_volume_accumulator"], venue)?;
    require_pda(&a[fee_config], &[b"fee_config", venue.as_ref()], &FEES)?;
    if !a[global].owned_by(venue)
        || !a[global_volume].owned_by(venue)
        || !a[fee_config].owned_by(&FEES)
    {
        return Err(Error::InvalidVenue.into());
    }
    validate_volume(&a[user_volume], treasury, venue)
}

fn in_recipients(data: &[u8], offset: usize, count: usize, recipient: &Address) -> bool {
    recipient != &Address::new_from_array([0; 32])
        && (0..count).any(|i| address_at(data, offset + i * 32, recipient))
}

#[inline(never)]
fn validate_curve(
    a: &[AccountView],
    mint: &AccountView,
    treasury: &AccountView,
    base: &AccountView,
    sharing: &AccountView,
    token_program: &AccountView,
) -> Result<(u64, u128, u64), ProgramError> {
    common(a, &PUMP, 0, 10, 11, 12, 13, 14, 15, treasury.address())?;
    require_address(&a[2], mint.address())?;
    require_address(&a[5], base.address())?;
    require_address(&a[6], treasury.address())?;
    require_program(&a[7], &pinocchio_system::ID)?;
    require_program(&a[8], token_program.address())?;
    require_pda(
        &a[9],
        &[b"creator-vault", sharing.address().as_ref()],
        &PUMP,
    )?;
    require_system(&a[9])?;
    require_rent(&a[9])?;
    require_pda(
        &a[16],
        &[b"bonding-curve-v2", mint.address().as_ref()],
        &PUMP,
    )?;
    validate_ata(
        &a[4],
        mint.address(),
        a[3].address(),
        token_program.address(),
    )?;
    let global = a[0].try_borrow()?;
    if global.get(..8) != Some(PUMP_GLOBAL_TAG.as_slice())
        || !(in_recipients(&global, 41, 1, a[1].address())
            || in_recipients(&global, 162, 7, a[1].address()))
        || !in_recipients(&global, 741, 8, a[17].address())
    {
        return Err(Error::InvalidVenue.into());
    }
    let (base, quote, real) = validate_curve_state(&a[3], mint.address(), sharing.address())?;
    if real > token_balance(&a[4])? {
        return Err(Error::InvalidVenue.into());
    }
    Ok((base, quote.into(), real))
}

#[allow(clippy::too_many_arguments)]
#[inline(never)]
fn validate_pool(
    a: &[AccountView],
    mint: &AccountView,
    treasury: &AccountView,
    base: &AccountView,
    wrapped: &AccountView,
    sharing: &AccountView,
    token_program: &AccountView,
) -> Result<(u64, u128, u64), ProgramError> {
    common(a, &AMM, 2, 15, 16, 19, 20, 21, 22, treasury.address())?;
    require_address(&a[1], treasury.address())?;
    require_address(&a[3], mint.address())?;
    require_address(&a[4], &WSOL)?;
    require_address(&a[5], base.address())?;
    require_address(&a[6], wrapped.address())?;
    require_program(&a[11], token_program.address())?;
    require_program(&a[12], &TOKEN)?;
    require_program(&a[13], &pinocchio_system::ID)?;
    require_program(&a[14], &ASSOCIATED_TOKEN)?;
    let pool_authority = pda(&[b"pool-authority", mint.address().as_ref()], &PUMP)?.0;
    require_pda(
        &a[0],
        &[
            b"pool",
            &[0, 0],
            pool_authority.as_ref(),
            mint.address().as_ref(),
            WSOL.as_ref(),
        ],
        &AMM,
    )?;
    require_pda(
        &a[18],
        &[b"creator_vault", sharing.address().as_ref()],
        &AMM,
    )?;
    require_pda(&a[23], &[b"pool-v2", mint.address().as_ref()], &AMM)?;
    // Pre-existing ATAs ensure upstream init_if_needed never charges treasury rent.
    validate_ata(
        &a[7],
        mint.address(),
        a[0].address(),
        token_program.address(),
    )?;
    validate_ata(&a[8], &WSOL, a[0].address(), &TOKEN)?;
    validate_ata(&a[10], &WSOL, a[9].address(), &TOKEN)?;
    validate_ata(&a[17], &WSOL, a[18].address(), &TOKEN)?;
    validate_ata(&a[25], &WSOL, a[24].address(), &TOKEN)?;
    let global = a[2].try_borrow()?;
    if global.get(..8) != Some(AMM_GLOBAL_TAG.as_slice())
        || !in_recipients(&global, 57, 8, a[9].address())
        || !in_recipients(&global, 643, 8, a[24].address())
    {
        return Err(Error::InvalidVenue.into());
    }
    if !a[0].owned_by(&AMM) {
        return Err(Error::InvalidVenue.into());
    }
    let pool = a[0].try_borrow()?;
    if pool.len() < 245
        || pool.get(..8) != Some(POOL_TAG.as_slice())
        || pool[9..11] != [0, 0]
        || !address_at(&pool, 11, &pool_authority)
        || !address_at(&pool, 43, mint.address())
        || !address_at(&pool, 75, &WSOL)
        || !address_at(&pool, 139, a[7].address())
        || !address_at(&pool, 171, a[8].address())
        || !address_at(&pool, 211, sharing.address())
    {
        return Err(Error::InvalidVenue.into());
    }
    if pool[243] != 0 || pool[244] != 0 {
        return Err(Error::UnsafeMode.into());
    }
    let zero_virtual_quote = [0; 16];
    let virtual_quote = if pool.len() == 245 {
        zero_virtual_quote.as_slice()
    } else {
        pool.get(245..261)
            .ok_or(ProgramError::InvalidAccountData)?
    };
    let base_reserve = token_balance(&a[7])?;
    let quote_reserve = effective_quote_reserve(token_balance(&a[8])?, virtual_quote)?;
    Ok((base_reserve, quote_reserve, base_reserve))
}

/// Only these two fixed exact-input CPIs may spend the treasury. The caller
/// cannot supply instructions, an input amount, recipient or signer seeds.
#[inline(never)]
pub fn buy(
    venue: u8,
    accounts: &[AccountView],
    amount: u64,
    minimum: u64,
    signers: &[Signer],
) -> ProgramResult {
    const PUMP_WRITABLE: [usize; 8] = [1, 3, 4, 5, 6, 9, 13, 17];
    const AMM_WRITABLE: [usize; 10] = [0, 1, 5, 6, 7, 8, 10, 17, 20, 25];
    let mut data = [0u8; 25];
    data[..8].copy_from_slice(if venue == 0 {
        &[56, 252, 116, 8, 158, 223, 205, 95]
    } else {
        &[198, 46, 21, 82, 180, 217, 232, 112]
    });
    data[8..16].copy_from_slice(&amount.to_le_bytes());
    data[16..24].copy_from_slice(&minimum.to_le_bytes());
    // OptionBool(false) is a single byte in the Pump IDLs.
    data[24] = 0;
    let metas: [InstructionAccount; 26] = core::array::from_fn(|index| {
        let account = &accounts[index.min(accounts.len() - 1)];
        let writable = if venue == 0 {
            PUMP_WRITABLE.contains(&index)
        } else {
            AMM_WRITABLE.contains(&index)
        };
        let signer = index == if venue == 0 { 6 } else { 1 };
        InstructionAccount::new(account.address(), writable, signer)
    });
    for (account, meta) in accounts.iter().zip(&metas) {
        if meta.is_writable {
            // invoke_signed also verifies privileges; fail before the CPI for a readable error.
            require_writable(account)?;
        }
    }
    invoke_signed_with_bounds::<26, _>(
        &InstructionView {
            program_id: if venue == 0 { &PUMP } else { &AMM },
            accounts: &metas[..accounts.len()],
            data: &data,
        },
        accounts,
        signers,
    )
}
