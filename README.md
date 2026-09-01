# Burn Supply

Burn Supply is a extension Solana program for a Pump.fun coin. It routes the coin's full Pump creator-fee share into a program treasury, lets anyone spend those fees to buy the coin and then burns every token bought in the same transaction.

## Addresses

These addresses are reserved and compiled into the current build. The program and coin are **not live yet**.

| | Address |
|---|---|
| Program | [`C1FqoguM1WredxG2SG6HfTAKwiby4iG2bZ41KSMbBURN`](https://solscan.io/account/C1FqoguM1WredxG2SG6HfTAKwiby4iG2bZ41KSMbBURN) |
| Token mint | [`hoWdqhtpuiQuXhw48HTPVJUSnThsbf84NhWQzz4pump`](https://pump.fun/coin/hoWdqhtpuiQuXhw48HTPVJUSnThsbf84NhWQzz4pump) |
| Upgrade authority | `3Ya1XBrSiJpRrWqvo8WxbQChfZp1wHxVn5SkmEY4DdM6` |

## How it works

1. The program is deployed before the coin is created.
2. Coin creation, the 100% creator-fee route, and contract activation happen atomically.
3. Before graduation, buybacks use the Pump.fun V2 bonding curve.
4. After graduation, buybacks use the canonical PumpSwap V2 pool.
5. Anyone can submit a buyback transaction. The program buys with available creator fees and burns the received tokens before the transaction can succeed.

“100%” refers to the coin's Pump creator-fee share. It does not include Pump protocol fees, liquidity-provider fees, or every fee paid by traders.

## Trust and control

The active fee-sharing account must name the program treasury as its only recipient at 10,000 basis points. The contract has no treasury withdrawal instruction and rejects caller-selected venues, recipients, spend amounts, or signer seeds.

The program remains upgradeable. `3Ya1XBrSiJpRrWqvo8WxbQChfZp1wHxVn5SkmEY4DdM6` retains upgrade authority and can replace the program code. Verify the current on-chain authority and executable hash before relying on it. This code has not received an independent security audit.

## Test and build

```sh
./contract/build.sh --test-fixture
cargo test --manifest-path contract/Cargo.toml --locked --features test-fixture
./contract/build.sh
solana-verify get-executable-hash contract/target/deploy/burned_fun.so
```

The reproducible production build uses the pinned public identities in [`contract/.cargo/config.toml`](contract/.cargo/config.toml). Its expected executable hash is:

```text
5c7d1d2c9772fb7bc86b2a0106cd45699389e051950a4f4226a5023bc565e213
```

No signer file belongs in this repository or in a public build environment.

## Security

Report suspected vulnerabilities privately through [GitHub Security Advisories](https://github.com/Trappyonchain/burn-supply/security/advisories/new). Do not open a public issue. See the [security policy](SECURITY.md) for scope and reporting guidance.

The production ELF embeds the same project, contact, policy, and source links using Solana's `security.txt` format. Inspect a local build with `query-security-txt contract/target/deploy/burned_fun.so`.

## Links

- Website: [burnsupply.fun](https://burnsupply.fun/)
- X: [@Trappyonchain](https://x.com/Trappyonchain)
- GitHub: [Trappyonchain/burn-supply](https://github.com/Trappyonchain/burn-supply)
