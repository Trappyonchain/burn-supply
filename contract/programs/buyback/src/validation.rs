use pinocchio::sysvars::{rent::Rent, Sysvar};
use pinocchio::{error::ProgramError, AccountView, Address, ProgramResult};

pub const PUMP: Address = Address::from_str_const("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
pub const AMM: Address = Address::from_str_const("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA");
pub const FEES: Address = Address::from_str_const("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ");
// Standard SPL Token is used only for WSOL, never for the coin being burned.
pub const TOKEN: Address = Address::from_str_const("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const TOKEN_2022: Address = pinocchio_token_2022::ID;
pub const ASSOCIATED_TOKEN: Address =
    Address::from_str_const("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
pub const WSOL: Address = Address::from_str_const("So11111111111111111111111111111111111111112");
pub const MAYHEM: Address = Address::from_str_const("MAyhSmzXzV1pTf7LsNkrNwkWKTo4ougAJ1PPg47MD4e");
pub const UPGRADEABLE_LOADER: Address =
    Address::from_str_const("BPFLoaderUpgradeab1e11111111111111111111111");
pub const STATE_LEN: usize = 112;
pub const STATE_TAG: &[u8; 8] = b"BURNFUN1";
pub const SHARING_TAG: [u8; 8] = [216, 74, 9, 0, 56, 140, 93, 75];
pub const VOLUME_TAG: [u8; 8] = [86, 255, 112, 14, 102, 53, 154, 250];
pub const MAX_SPEND: u64 = 100_000_000; // 0.1 SOL per execution.
pub const MAX_DEADLINE_SLOTS: u64 = 150;

#[repr(u32)]
#[derive(Clone, Copy, Debug)]
pub enum Error {
    InvalidMint = 6000,
    InvalidVault,
    InvalidSharing,
    InvalidVenue,
    UnsafeMode,
    AccountNotPrepared,
    Expired,
    NothingToBuy,
    Slippage,
    InvalidSpend,
    BurnFailed,
    InvalidDeployment,
    InvalidMetadata,
    SetupCapExceeded,
}

impl From<Error> for ProgramError {
    fn from(value: Error) -> Self {
        ProgramError::Custom(value as u32)
    }
}

pub fn u64_at(data: &[u8], offset: usize) -> Result<u64, ProgramError> {
    Ok(u64::from_le_bytes(
        data.get(offset..offset + 8)
            .ok_or(ProgramError::InvalidAccountData)?
            .try_into()
            .unwrap(),
    ))
}

pub fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

pub fn address_at(data: &[u8], offset: usize, expected: &Address) -> bool {
    data.get(offset..offset + 32) == Some(expected.as_ref())
}

pub fn effective_quote_reserve(real: u64, virtual_le: &[u8]) -> Result<u128, ProgramError> {
    let virtual_quote = i128::from_le_bytes(
        virtual_le
            .try_into()
            .map_err(|_| ProgramError::InvalidAccountData)?,
    );
    u128::try_from(
        i128::from(real)
            .checked_add(virtual_quote)
            .filter(|amount| *amount > 0)
            .ok_or(Error::UnsafeMode)?,
    )
    .map_err(|_| Error::UnsafeMode.into())
}

pub fn pda(seeds: &[&[u8]], program: &Address) -> Result<(Address, u8), ProgramError> {
    Address::try_find_program_address(seeds, program).ok_or(ProgramError::InvalidSeeds)
}

pub fn require_address(account: &AccountView, expected: &Address) -> ProgramResult {
    if account.address() != expected {
        return Err(ProgramError::InvalidSeeds);
    }
    Ok(())
}

pub fn require_pda(account: &AccountView, seeds: &[&[u8]], program: &Address) -> ProgramResult {
    require_address(account, &pda(seeds, program)?.0)
}

pub fn require_program(account: &AccountView, program: &Address) -> ProgramResult {
    if account.address() != program || !account.executable() {
        return Err(ProgramError::IncorrectProgramId);
    }
    Ok(())
}

pub fn require_writable(account: &AccountView) -> ProgramResult {
    if !account.is_writable() {
        return Err(ProgramError::InvalidAccountData);
    }
    Ok(())
}

pub fn require_system(account: &AccountView) -> ProgramResult {
    if !account.owned_by(&pinocchio_system::ID) || account.data_len() != 0 || account.executable() {
        return Err(ProgramError::IllegalOwner);
    }
    Ok(())
}

pub fn require_rent(account: &AccountView) -> ProgramResult {
    if account.lamports() < Rent::get()?.try_minimum_balance(account.data_len())? {
        return Err(Error::AccountNotPrepared.into());
    }
    Ok(())
}

pub fn require_deployment(
    program_id: &Address,
    program: &AccountView,
    program_data: &AccountView,
    upgrade_authority: &Address,
) -> ProgramResult {
    require_program(program, program_id)?;
    require_pda(program_data, &[program_id.as_ref()], &UPGRADEABLE_LOADER)?;
    if !program.owned_by(&UPGRADEABLE_LOADER)
        || !program_data.owned_by(&UPGRADEABLE_LOADER)
        || program.data_len() != 36
        || program_data.data_len() < 45
        || program_data.executable()
    {
        return Err(Error::InvalidDeployment.into());
    }
    let state = program.try_borrow()?;
    let deployed = program_data.try_borrow()?;
    // Loader-v3 Program(2) must point to ProgramData(3). The program may be
    // immutable or retain only the setup authority compiled into this build.
    if u32_at(&state, 0) != Some(2)
        || !address_at(&state, 4, program_data.address())
        || u32_at(&deployed, 0) != Some(3)
        || (deployed[12] != 0
            && (deployed[12] != 1 || !address_at(&deployed, 13, upgrade_authority)))
    {
        return Err(Error::InvalidDeployment.into());
    }
    Ok(())
}

pub fn validate_curve_state(
    account: &AccountView,
    mint: &Address,
    creator: &Address,
) -> Result<(u64, u64, u64), ProgramError> {
    require_pda(account, &[b"bonding-curve", mint.as_ref()], &PUMP)?;
    if !account.owned_by(&PUMP) {
        return Err(Error::InvalidVenue.into());
    }
    let curve = account.try_borrow()?;
    if curve.len() < 83
        || curve.get(..8) != Some([23, 183, 248, 55, 96, 216, 172, 96].as_slice())
        || curve[48] != 0
        || !address_at(&curve, 49, creator)
    {
        return Err(Error::InvalidVenue.into());
    }
    let quote = curve.get(83..115).unwrap_or(&curve[83..]);
    if curve[81] != 0 || curve[82] != 0 || (quote.iter().any(|b| *b != 0) && quote != WSOL.as_ref())
    {
        return Err(Error::UnsafeMode.into());
    }
    Ok((u64_at(&curve, 8)?, u64_at(&curve, 16)?, u64_at(&curve, 24)?))
}

/// A mint with no authority cannot later turn on a transfer fee, hook or delegate.
/// Only metadata extensions are accepted; unknown future extensions fail closed.
pub fn validate_mint_data(data: &[u8]) -> Result<u8, ProgramError> {
    if data.len() < 82
        || data.len() == 355
        || u32_at(data, 0) != Some(0)
        || data[45] != 1
        || u32_at(data, 46) != Some(0)
        || data[44] > 18
    {
        return Err(Error::InvalidMint.into());
    }
    if data.len() == 82 {
        return Ok(data[44]);
    }
    if data.len() < 166 || data[82..165].iter().any(|b| *b != 0) || data[165] != 1 {
        return Err(Error::InvalidMint.into());
    }
    validate_extensions(&data[166..], true)?;
    Ok(data[44])
}

fn validate_extensions(data: &[u8], mint: bool) -> ProgramResult {
    let mut cursor = 0;
    let mut seen = 0u32;
    while cursor < data.len() {
        // Token-2022 permits unused zeroed allocation at the end.
        if data[cursor..].iter().all(|b| *b == 0) {
            return Ok(());
        }
        let header = data.get(cursor..cursor + 4).ok_or(Error::InvalidMint)?;
        let kind = u16::from_le_bytes([header[0], header[1]]);
        let length = u16::from_le_bytes([header[2], header[3]]) as usize;
        let allowed = if mint {
            (kind == 18 && length == 64) || (kind == 19 && length >= 80)
        } else {
            kind == 7 && length == 0
        }; // ImmutableOwner on Token-2022 ATAs.
        if !allowed || seen & (1 << kind) != 0 {
            return Err(Error::InvalidMint.into());
        }
        seen |= 1 << kind;
        cursor = cursor
            .checked_add(4 + length)
            .filter(|n| *n <= data.len())
            .ok_or(Error::InvalidMint)?;
    }
    Ok(())
}

pub fn validate_mint(account: &AccountView, program: &AccountView) -> Result<u8, ProgramError> {
    require_program(program, &TOKEN_2022)?;
    if !account.owned_by(&TOKEN_2022) || account.address() == &WSOL {
        return Err(Error::InvalidMint.into());
    }
    validate_mint_data(&account.try_borrow()?)
}

pub fn token_balance(account: &AccountView) -> Result<u64, ProgramError> {
    u64_at(&account.try_borrow()?, 64)
}

pub fn validate_ata(
    account: &AccountView,
    mint: &Address,
    owner: &Address,
    program: &Address,
) -> ProgramResult {
    require_pda(
        account,
        &[owner.as_ref(), program.as_ref(), mint.as_ref()],
        &ASSOCIATED_TOKEN,
    )?;
    if !account.owned_by(program) {
        return Err(Error::InvalidVault.into());
    }
    let data = account.try_borrow()?;
    if data.len() < 165
        || data.len() == 355
        || !address_at(&data, 0, mint)
        || !address_at(&data, 32, owner)
        || data[108] != 1
        || u32_at(&data, 72) != Some(0)
        || u32_at(&data, 129) != Some(0)
        || u32_at(&data, 109) != Some(u32::from(mint == &WSOL))
    {
        return Err(Error::InvalidVault.into());
    }
    if data.len() > 165 {
        if program != &TOKEN_2022 || data[165] != 2 {
            return Err(Error::InvalidVault.into());
        }
        validate_extensions(&data[166..], false)?;
    }
    drop(data);
    require_rent(account)
}

pub fn validate_sharing_data(data: &[u8], mint: &Address, treasury: &Address) -> ProgramResult {
    if data.get(..8) != Some(SHARING_TAG.as_slice())
        || data.get(9) != Some(&2)
        || data.get(10) != Some(&1)
        || !address_at(data, 11, mint)
        || data.get(75) != Some(&1)
        || u32_at(data, 76) != Some(1)
        || !address_at(data, 80, treasury)
        || data.get(112..114) != Some(10_000u16.to_le_bytes().as_slice())
    {
        return Err(Error::InvalidSharing.into());
    }
    Ok(())
}

pub fn validate_sharing(
    account: &AccountView,
    mint: &Address,
    treasury: &Address,
) -> ProgramResult {
    require_pda(account, &[b"sharing-config", mint.as_ref()], &FEES)?;
    if !account.owned_by(&FEES) {
        return Err(Error::InvalidSharing.into());
    }
    validate_sharing_data(&account.try_borrow()?, mint, treasury)
}

pub fn validate_volume(
    account: &AccountView,
    treasury: &Address,
    venue: &Address,
) -> ProgramResult {
    require_pda(
        account,
        &[b"user_volume_accumulator", treasury.as_ref()],
        venue,
    )?;
    let minimum = if venue == &PUMP { 106 } else { 90 };
    if !account.owned_by(venue) || account.data_len() < minimum {
        return Err(Error::AccountNotPrepared.into());
    }
    let data = account.try_borrow()?;
    if data.get(..8) != Some(VOLUME_TAG.as_slice()) || !address_at(&data, 8, treasury) {
        return Err(Error::AccountNotPrepared.into());
    }
    drop(data);
    require_rent(account)
}

pub fn validate_deadline(current: u64, deadline: u64) -> ProgramResult {
    if deadline < current || deadline - current > MAX_DEADLINE_SLOTS {
        return Err(Error::Expired.into());
    }
    Ok(())
}

/// An on-chain spot-reserve bound, not an oracle or TWAP. Fees plus slippage
/// cannot consume more than 5% of the fee-free constant-product quote.
pub fn trade_limits(
    available: u64,
    base: u64,
    quote: u128,
    real_base: u64,
) -> Result<(u64, u64), ProgramError> {
    let spend = available
        .min(MAX_SPEND)
        .min(u64::try_from(quote / 100).unwrap_or(u64::MAX));
    if spend == 0 || base == 0 || quote == 0 {
        return Err(Error::NothingToBuy.into());
    }
    let ideal = (spend as u128 * base as u128 / (quote + spend as u128)).min(real_base as u128);
    let floor = ideal * 9_500 / 10_000;
    if floor == 0 {
        return Err(Error::NothingToBuy.into());
    }
    Ok((spend, floor as u64))
}
