#![cfg_attr(any(target_os = "solana", target_arch = "bpf"), no_std)]
#![allow(unexpected_cfgs)]

#[cfg(any(target_os = "solana", target_arch = "bpf"))]
mod launch;

// A local test venue only. Never deploy this program or treat it as Pump.
#[cfg(any(target_os = "solana", target_arch = "bpf"))]
pinocchio::program_entrypoint!(process_instruction, 26);
#[cfg(any(target_os = "solana", target_arch = "bpf"))]
pinocchio::no_allocator!();
#[cfg(any(target_os = "solana", target_arch = "bpf"))]
pinocchio::nostd_panic_handler!();

#[cfg(any(target_os = "solana", target_arch = "bpf"))]
fn process_instruction(
    program: &pinocchio::Address,
    accounts: &mut [pinocchio::AccountView],
    data: &[u8],
) -> pinocchio::ProgramResult {
    use pinocchio::{
        cpi::{Seed, Signer},
        error::ProgramError,
        Address,
    };
    use pinocchio_system::instructions::{Assign, Transfer};
    use pinocchio_token_2022::instructions::{AuthorityType, SetAuthority, TransferChecked};
    if let Some(result) = launch::handle(program, accounts, data) {
        return result;
    }
    let amount_at = |data: &[u8], offset: usize| -> u64 {
        u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap())
    };
    if data.len() != 25 {
        return Err(ProgramError::InvalidInstructionData);
    }
    let amount = amount_at(data, 8);
    let min_out = amount_at(data, 16);
    let is_pump = data[..8] == [56, 252, 116, 8, 158, 223, 205, 95];
    if is_pump {
        if accounts.len() != 18 {
            return Err(ProgramError::NotEnoughAccountKeys);
        }
        let curve_data = accounts[3].try_borrow()?;
        let base = amount_at(&curve_data, 8);
        let quote = amount_at(&curve_data, 16);
        let real = amount_at(&curve_data, 24);
        let mode = curve_data[115];
        drop(curve_data);
        let output = if mode == 1 {
            min_out.saturating_sub(1)
        } else {
            ((amount as u128 * base as u128 / (quote as u128 + amount as u128)) as u64).min(real)
        };
        let input = match mode {
            2 => amount + 1,
            3 => amount / 2,
            _ => amount,
        };
        Transfer {
            from: &accounts[6],
            to: &accounts[3],
            lamports: input,
        }
        .invoke()?;
        if mode == 3 {
            Transfer {
                from: &accounts[6],
                to: &accounts[13],
                lamports: 1,
            }
            .invoke()?;
        }
        let (_, bump) = Address::find_program_address(
            &[b"bonding-curve", accounts[2].address().as_ref()],
            program,
        );
        let bump = [bump];
        let seeds = [
            Seed::from(b"bonding-curve".as_slice()),
            Seed::from(accounts[2].address().as_ref()),
            Seed::from(&bump),
        ];
        let decimals = accounts[2].try_borrow()?[44];
        TransferChecked {
            from: &accounts[4],
            mint: &accounts[2],
            to: &accounts[5],
            authority: &accounts[3],
            amount: output,
            decimals,
            token_program: accounts[8].address(),
        }
        .invoke_signed(&[Signer::from(&seeds)])?;
        let mut curve_data = accounts[3].try_borrow_mut()?;
        for (offset, value) in [(8, base - output), (16, quote + input), (24, real - output)] {
            curve_data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        }
        drop(curve_data);
        if mode == 4 {
            Assign {
                account: &accounts[6],
                owner: program,
            }
            .invoke()?;
        }
        if mode == 5 {
            SetAuthority {
                account: &accounts[5],
                authority: &accounts[6],
                authority_type: AuthorityType::CloseAccount,
                new_authority: Some(program),
                token_program: accounts[8].address(),
            }
            .invoke()?;
        }
    } else {
        if data[..8] != [198, 46, 21, 82, 180, 217, 232, 112] || accounts.len() != 26 {
            return Err(ProgramError::InvalidInstructionData);
        }
        let base = amount_at(&accounts[7].try_borrow()?, 64);
        let quote = amount_at(&accounts[8].try_borrow()?, 64);
        let output = (amount as u128 * base as u128 / (quote as u128 + amount as u128)) as u64;
        TransferChecked {
            from: &accounts[6],
            mint: &accounts[4],
            to: &accounts[8],
            authority: &accounts[1],
            amount,
            decimals: 9,
            token_program: accounts[12].address(),
        }
        .invoke()?;
        let pool_data = accounts[0].try_borrow()?;
        let creator: [u8; 32] = pool_data[11..43].try_into().unwrap();
        drop(pool_data);
        let (_, bump) = Address::find_program_address(
            &[
                b"pool",
                &[0, 0],
                &creator,
                accounts[3].address().as_ref(),
                accounts[4].address().as_ref(),
            ],
            program,
        );
        let bump = [bump];
        let seeds = [
            Seed::from(b"pool".as_slice()),
            Seed::from([0, 0].as_slice()),
            Seed::from(creator.as_slice()),
            Seed::from(accounts[3].address().as_ref()),
            Seed::from(accounts[4].address().as_ref()),
            Seed::from(&bump),
        ];
        let decimals = accounts[3].try_borrow()?[44];
        TransferChecked {
            from: &accounts[7],
            mint: &accounts[3],
            to: &accounts[5],
            authority: &accounts[0],
            amount: output,
            decimals,
            token_program: accounts[11].address(),
        }
        .invoke_signed(&[Signer::from(&seeds)])?;
    }
    Ok(())
}
