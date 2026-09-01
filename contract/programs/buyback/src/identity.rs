use pinocchio::Address;

// Public local-VM fixtures only. Production builds require the operator's
// explicitly supplied public addresses; no key material belongs in this crate.
#[cfg(feature = "test-fixture")]
pub const AUTHORITY: Address = Address::new_from_array([3; 32]);
#[cfg(feature = "test-fixture")]
pub const MINT: Address = Address::new_from_array([9; 32]);

#[cfg(not(feature = "test-fixture"))]
pub const AUTHORITY: Address = Address::from_str_const(env!("BURN_AUTHORITY_ADDRESS"));
#[cfg(not(feature = "test-fixture"))]
pub const MINT: Address = Address::from_str_const(env!("BURN_MINT_ADDRESS"));
