use crate::{identity, validation::*};
use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::{rent::Rent, Sysvar},
    AccountView, Address, ProgramResult,
};
use pinocchio_system::{create_account_with_minimum_balance_signed, instructions::Transfer};

const MAX_SETUP_SPEND: u64 = 100_000_000;
// @pump-fun/pump-sdk 1.36.0 src/sdk.ts: BONDING_CURVE_NEW_SIZE.
const BONDING_CURVE_NEW_SIZE: usize = 151;

fn authority(payer: &AccountView, mint: &AccountView) -> ProgramResult {
    if !payer.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    require_address(payer, &identity::AUTHORITY)?;
    require_address(mint, &identity::MINT)?;
    require_system(payer)
}

// The operator brackets the SDK's token creation and permanent fee assignment
// with begin/finish in ONE transaction. Pending state can never spend fees.
pub(crate) fn begin(program_id: &Address, accounts: &mut [AccountView]) -> ProgramResult {
    let [payer, config, treasury, mint, system, program, program_data] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    authority(payer, mint)?;
    for account in [&*payer, &*config, &*treasury] {
        require_writable(account)?;
    }
    require_program(system, &pinocchio_system::ID)?;
    require_deployment(program_id, program, program_data, &identity::AUTHORITY)?;
    require_system(mint)?;
    require_system(config)?;
    require_system(treasury)?;
    let (expected_config, config_bump) = pda(&[b"buyback", mint.address().as_ref()], program_id)?;
    let (expected_treasury, treasury_bump) =
        pda(&[b"treasury", mint.address().as_ref()], program_id)?;
    require_address(config, &expected_config)?;
    require_address(treasury, &expected_treasury)?;
    // Runtime transaction fees were already debited. Include every setup rent
    // payment, including this instruction's own payments, in the recorded debit.
    let balance_before = payer.lamports();
    let bump = [config_bump];
    let seeds = [
        Seed::from(b"buyback".as_slice()),
        Seed::from(mint.address().as_ref()),
        Seed::from(&bump),
    ];
    create_account_with_minimum_balance_signed(
        config,
        STATE_LEN,
        program_id,
        payer,
        None,
        &[Signer::from(&seeds)],
    )?;
    let topup = Rent::get()?
        .try_minimum_balance(0)?
        .saturating_sub(treasury.lamports());
    if topup > 0 {
        Transfer {
            from: payer,
            to: treasury,
            lamports: topup,
        }
        .invoke()?;
    }
    let mut state = config.try_borrow_mut()?;
    state.fill(0);
    state[..8].copy_from_slice(STATE_TAG);
    state[8..40].copy_from_slice(mint.address().as_ref());
    state[40..72].copy_from_slice(TOKEN_2022.as_ref());
    state[72] = treasury_bump;
    state[73] = config_bump;
    state[104..112].copy_from_slice(&balance_before.to_le_bytes());
    Ok(())
}

pub(crate) fn finish(program_id: &Address, accounts: &mut [AccountView]) -> ProgramResult {
    let [payer, config, treasury, mint, base, wrapped, token, system, sharing, program, program_data, curve, creator_vault, pump_volume, amm_volume] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    authority(payer, mint)?;
    for account in [&*payer, &*config, &*treasury] {
        require_writable(account)?;
    }
    require_program(system, &pinocchio_system::ID)?;
    require_deployment(program_id, program, program_data, &identity::AUTHORITY)?;
    require_system(treasury)?;
    require_rent(treasury)?;
    let (expected_config, config_bump) = pda(&[b"buyback", mint.address().as_ref()], program_id)?;
    let (expected_treasury, treasury_bump) =
        pda(&[b"treasury", mint.address().as_ref()], program_id)?;
    require_address(config, &expected_config)?;
    require_address(treasury, &expected_treasury)?;
    if !config.owned_by(program_id) || config.data_len() != STATE_LEN {
        return Err(ProgramError::IllegalOwner);
    }
    let balance_before = {
        let state = config.try_borrow()?;
        if state.get(..8) != Some(STATE_TAG.as_slice())
            || !address_at(&state, 8, mint.address())
            || !address_at(&state, 40, &TOKEN_2022)
            || state[72] != treasury_bump
            || state[73] != config_bump
            || state[75] != 0
        {
            return Err(ProgramError::InvalidAccountData);
        }
        u64_at(&state, 104)?
    };
    let decimals = validate_mint(mint, token)?;
    validate_ata(base, mint.address(), treasury.address(), &TOKEN_2022)?;
    validate_ata(wrapped, &WSOL, treasury.address(), &TOKEN)?;
    validate_sharing(sharing, mint.address(), treasury.address())?;
    if curve.data_len() < BONDING_CURVE_NEW_SIZE {
        return Err(Error::AccountNotPrepared.into());
    }
    require_rent(curve)?;
    if validate_curve_state(curve, mint.address(), sharing.address())?.2 == 0 {
        return Err(Error::InvalidVenue.into());
    }
    require_pda(
        creator_vault,
        &[b"creator-vault", sharing.address().as_ref()],
        &PUMP,
    )?;
    require_system(creator_vault)?;
    require_rent(creator_vault)?;
    validate_volume(pump_volume, treasury.address(), &PUMP)?;
    validate_volume(amm_volume, treasury.address(), &AMM)?;
    // UpdateFeeSharesV2 can sweep pre-launch donations to the initial creator.
    // A refund is zero net wallet spending, not a reason to block activation.
    let spent = balance_before.saturating_sub(payer.lamports());
    if spent > MAX_SETUP_SPEND {
        return Err(Error::SetupCapExceeded.into());
    }
    let mut state = config.try_borrow_mut()?;
    state[74] = decimals;
    state[75] = 1;
    state[104..112].fill(0);
    Ok(())
}
