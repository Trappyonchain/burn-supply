#![cfg_attr(any(target_os = "solana", target_arch = "bpf"), no_std)]
#![allow(unexpected_cfgs)]

mod activation;
mod identity;
pub mod validation;
mod venues;

use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::{clock::Clock, rent::Rent, Sysvar},
    AccountView, Address, ProgramResult,
};
use pinocchio_system::instructions::Transfer;
use pinocchio_token_2022::instructions::{BurnChecked, SyncNative};
use validation::*;

#[cfg(feature = "entrypoint")]
solana_security_txt::security_txt! {
    name: "Burn Supply",
    project_url: "https://burnsupply.fun",
    contacts: "link:https://github.com/Trappyonchain/burn-supply/security/advisories/new,twitter:@Trappyonchain",
    policy: "https://github.com/Trappyonchain/burn-supply/security/policy",
    preferred_languages: "en",
    source_code: "https://github.com/Trappyonchain/burn-supply"
}

#[cfg(all(feature = "entrypoint", any(target_os = "solana", target_arch = "bpf")))]
pinocchio::program_entrypoint!(process_instruction, 40);
#[cfg(all(feature = "entrypoint", any(target_os = "solana", target_arch = "bpf")))]
pinocchio::no_allocator!();
#[cfg(all(feature = "entrypoint", any(target_os = "solana", target_arch = "bpf")))]
pinocchio::nostd_panic_handler!();

pub fn process_instruction(
    program_id: &Address,
    accounts: &mut [AccountView],
    data: &[u8],
) -> ProgramResult {
    match data {
        [0] => activation::begin(program_id, accounts),
        [1, venue, ..] if data.len() == 18 => execute(
            program_id,
            accounts,
            *venue,
            u64_at(data, 2)?,
            u64_at(data, 10)?,
        ),
        [2] => activation::finish(program_id, accounts),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

#[inline(never)]
fn execute(
    program_id: &Address,
    accounts: &mut [AccountView],
    venue: u8,
    min_out: u64,
    deadline: u64,
) -> ProgramResult {
    if !matches!(venue, 0 | 1) || min_out == 0 {
        return Err(ProgramError::InvalidInstructionData);
    }
    if accounts.len() != 8 + if venue == 0 { 18 } else { 26 } {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    let (prefix, venue_accounts) = accounts.split_at_mut(8);
    let [config, treasury, mint, base_vault, wsol_vault, sharing, token_program, system_program] =
        prefix
    else {
        unreachable!()
    };
    for account in [&*config, &*treasury, &*mint, &*base_vault, &*wsol_vault] {
        require_writable(account)?;
    }
    let current_slot = Clock::get()?.slot;
    validate_deadline(current_slot, deadline)?;
    require_program(system_program, &pinocchio_system::ID)?;
    require_address(mint, &identity::MINT)?;
    require_system(treasury)?;
    require_rent(treasury)?;
    let decimals = validate_mint(mint, token_program)?;
    let (config_address, config_bump) = pda(&[b"buyback", mint.address().as_ref()], program_id)?;
    require_address(config, &config_address)?;
    let (treasury_address, treasury_bump) =
        pda(&[b"treasury", mint.address().as_ref()], program_id)?;
    require_address(treasury, &treasury_address)?;
    if !config.owned_by(program_id) || config.data_len() != STATE_LEN {
        return Err(ProgramError::IllegalOwner);
    }
    {
        let state = config.try_borrow()?;
        if state.get(..8) != Some(STATE_TAG.as_slice())
            || !address_at(&state, 8, mint.address())
            || !address_at(&state, 40, token_program.address())
            || state[72] != treasury_bump
            || state[73] != config_bump
            || state[74] != decimals
            || state[75] != 1
        {
            return Err(ProgramError::InvalidAccountData);
        }
    }
    validate_ata(
        base_vault,
        mint.address(),
        treasury.address(),
        token_program.address(),
    )?;
    validate_ata(wsol_vault, &WSOL, treasury.address(), &TOKEN)?;
    validate_sharing(sharing, mint.address(), treasury.address())?;
    let (base_reserve, quote_reserve, real_base) = venues::validate(
        venue,
        venue_accounts,
        mint,
        treasury,
        base_vault,
        wsol_vault,
        sharing,
        token_program,
    )?;
    let rent = Rent::get()?.try_minimum_balance(0)?;
    let native_before = treasury.lamports();
    let base_before = token_balance(base_vault)?;
    let bump = [treasury_bump];
    let seeds = [
        Seed::from(b"treasury".as_slice()),
        Seed::from(mint.address().as_ref()),
        Seed::from(&bump),
    ];
    let signers = [Signer::from(&seeds)];
    // All wrapped fees stay in the treasury ATA. Before graduation, Pump's
    // native fees are spent directly; any donated WSOL waits for the AMM.
    if venue == 1 {
        SyncNative {
            native_token: wsol_vault,
            token_program: &TOKEN,
        }
        .invoke()?;
    }
    let wrapped_before = if venue == 1 {
        token_balance(wsol_vault)?
    } else {
        0
    };
    let available = native_before
        .checked_sub(rent)
        .and_then(|n| n.checked_add(wrapped_before))
        .ok_or(ProgramError::ArithmeticOverflow)?;
    let (budget, floor) = trade_limits(available, base_reserve, quote_reserve, real_base)?;
    let required_output = min_out.max(floor);
    if venue == 1 && wrapped_before < budget {
        Transfer {
            from: treasury,
            to: wsol_vault,
            lamports: budget - wrapped_before,
        }
        .invoke_signed(&signers)?;
        SyncNative {
            native_token: wsol_vault,
            token_program: &TOKEN,
        }
        .invoke()?;
    }
    let volume_index = if venue == 0 { 13 } else { 20 };
    let volume_lamports = venue_accounts[volume_index].lamports();
    let account_lengths: [usize; 26] =
        core::array::from_fn(|index| venue_accounts.get(index).map_or(0, AccountView::data_len));
    venues::buy(venue, venue_accounts, budget, required_output, &signers)?;
    // Future upstream reallocations must be paid separately by the caller;
    // even a partial fill may not use spare budget to pay account rent.
    if venue_accounts[volume_index].lamports() != volume_lamports
        || venue_accounts
            .iter()
            .zip(account_lengths)
            .any(|(account, length)| account.data_len() != length)
    {
        return Err(Error::AccountNotPrepared.into());
    }
    // A venue gets the treasury signer only for the swap. It may not retain
    // control by assigning the treasury or setting token delegates/authorities.
    require_system(treasury)?;
    validate_ata(
        base_vault,
        mint.address(),
        treasury.address(),
        token_program.address(),
    )?;
    validate_ata(wsol_vault, &WSOL, treasury.address(), &TOKEN)?;
    let base_after = token_balance(base_vault)?;
    if base_after.checked_sub(base_before).ok_or(Error::Slippage)? < required_output {
        return Err(Error::Slippage.into());
    }
    let wrapped_after = if venue == 1 {
        token_balance(wsol_vault)?
    } else {
        0
    };
    let value_before = native_before as u128 + wrapped_before as u128;
    let value_after = treasury.lamports() as u128 + wrapped_after as u128;
    let spent = value_before
        .checked_sub(value_after)
        .ok_or(Error::InvalidSpend)?;
    if treasury.lamports() < rent || spent == 0 || spent > budget as u128 {
        return Err(Error::InvalidSpend.into());
    }
    let supply_before = u64_at(&mint.try_borrow()?, 36)?;
    BurnChecked {
        account: base_vault,
        mint,
        authority: treasury,
        amount: base_after,
        decimals,
        token_program: token_program.address(),
    }
    .invoke_signed(&signers)?;
    if token_balance(base_vault)? != 0
        || u64_at(&mint.try_borrow()?, 36)?
            != supply_before
                .checked_sub(base_after)
                .ok_or(Error::BurnFailed)?
    {
        return Err(Error::BurnFailed.into());
    }
    let mut state = config.try_borrow_mut()?;
    for (offset, amount) in [(80, spent as u64), (88, base_after), (96, 1)] {
        let next = u64_at(&state, offset)?
            .checked_add(amount)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        state[offset..offset + 8].copy_from_slice(&next.to_le_bytes());
    }
    state[104..112].copy_from_slice(&current_slot.to_le_bytes());
    Ok(())
}
