use serde::{Deserialize, Serialize};

/// Raydium CPMM Pool State layout in memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaydiumCpmmPoolState {
    pub pool_pubkey: String,
    pub base_mint: String,
    pub quote_mint: String, // Typically WSOL
    pub base_reserve_lamports: u64,
    pub quote_reserve_lamports: u64,
    pub lp_supply: u64,
    pub open_time: u64,
    pub fee_bps: u64,
}

impl RaydiumCpmmPoolState {
    /// Zero-copy decoding of Raydium CPMM / AMM v4 Account bytes
    /// Standard Raydium CPMM layout:
    /// Status: u64 (0..8)
    /// Nonce: u64 (8..16)
    /// MaxOrder: u64 (16..24)
    /// Depth: u64 (24..32)
    /// BaseDecimal: u64 (32..40)
    /// QuoteDecimal: u64 (40..48)
    /// State: u64 (48..56)
    /// ResetFlag: u64 (56..64)
    /// MinSize: u64 (64..72)
    /// VolMaxCutRatio: u64 (72..80)
    /// AmountWave: u64 (80..88)
    /// BaseLotSize: u64 (88..96)
    /// QuoteLotSize: u64 (96..104)
    /// MinPriceMultiplier: u64 (104..112)
    /// MaxPriceMultiplier: u64 (112..120)
    /// SystemDecimalValue: u64 (120..128)
    /// MinSeparateNumerator: u64 (128..136)
    /// MinSeparateDenominator: u64 (136..144)
    /// TradeFeeNumerator: u64 (144..152)
    /// TradeFeeDenominator: u64 (152..160)
    /// PnlNumerator: u64 (160..168)
    /// PnlDenominator: u64 (168..176)
    /// SwapFeeNumerator: u64 (176..184)
    /// SwapFeeDenominator: u64 (184..192)
    /// BaseNeedTakePnl: u64 (192..200)
    /// QuoteNeedTakePnl: u64 (200..208)
    /// QuoteTotalPnl: u64 (208..216)
    /// BaseTotalPnl: u64 (216..224)
    /// PoolOpenTime: u64 (224..232)
    /// PunishPcnt: u64 (232..240)
    /// PunishCoinPcnt: u64 (240..248)
    /// OrderbookToInitTime: u64 (248..256)
    /// SwapBaseInAmount: u128 (256..272)
    /// SwapQuoteOutAmount: u128 (272..288)
    /// SwapBase2QuoteFee: u64 (288..296)
    /// SwapQuoteInAmount: u128 (296..312)
    /// SwapBaseOutAmount: u128 (312..328)
    /// SwapQuote2BaseFee: u64 (328..336)
    /// BaseVault: Pubkey (336..368)
    /// QuoteVault: Pubkey (368..400)
    /// BaseMint: Pubkey (400..432)
    /// QuoteMint: Pubkey (432..464)
    /// LpMint: Pubkey (464..496)
    /// OpenOrders: Pubkey (496..528)
    /// MarketId: Pubkey (528..560)
    /// MarketProgramId: Pubkey (560..592)
    /// TargetOrders: Pubkey (592..624)
    /// WithdrawQueue: Pubkey (624..656)
    /// LpVault: Pubkey (656..688)
    /// Owner: Pubkey (688..720)
    /// LpReserve: u64 (720..728)
    pub fn decode_reserves_from_raw_bytes(data: &[u8]) -> Option<(u64, u64)> {
        // Minimal safety check on account data length
        if data.len() < 752 {
            // For standard Raydium CPMM Anchor accounts (~632-752 bytes)
            // Attempt fallback parsing for compact Raydium CPMM v1/v2 layout
            if data.len() >= 128 {
                let r_base = u64::from_le_bytes(data[40..48].try_into().ok()?);
                let r_quote = u64::from_le_bytes(data[48..56].try_into().ok()?);
                if r_base > 0 && r_quote > 0 {
                    return Some((r_base, r_quote));
                }
            }
            return None;
        }

        // Raydium AMM v4 Standard Layout
        // We parse quote_reserve (SOL) and base_reserve (Token)
        // Check standard vault delta or direct reserve offsets
        let quote_res = u64::from_le_bytes(data[192..200].try_into().ok()?);
        let base_res = u64::from_le_bytes(data[200..208].try_into().ok()?);

        if quote_res > 0 && base_res > 0 {
            Some((base_res, quote_res))
        } else {
            // Alternative reserve offset inspection
            let alt_base = u64::from_le_bytes(data[720..728].try_into().ok()?);
            let alt_quote = u64::from_le_bytes(data[728..736].try_into().ok()?);
            if alt_base > 0 && alt_quote > 0 {
                Some((alt_base, alt_quote))
            } else {
                None
            }
        }
    }

    /// Calculate Constant-Product Output with 0.25% (25 bps) Raydium LP fee
    pub fn calculate_swap_out(
        reserve_in: u64,
        reserve_out: u64,
        amount_in: u64,
        fee_bps: u64,
    ) -> u64 {
        if reserve_in == 0 || reserve_out == 0 || amount_in == 0 {
            return 0;
        }
        let fee_multiplier = 10_000u128.saturating_sub(fee_bps as u128);
        let amount_in_with_fee = (amount_in as u128) * fee_multiplier;
        let numerator = amount_in_with_fee * (reserve_out as u128);
        let denominator = ((reserve_in as u128) * 10_000u128) + amount_in_with_fee;
        if denominator == 0 {
            return 0;
        }
        (numerator / denominator) as u64
    }

    /// Calculate Spot Price in SOL per Token
    pub fn calculate_price_sol(r_sol_lamports: u64, r_tok_raw: u64, tok_decimals: u8) -> f64 {
        if r_tok_raw == 0 || r_sol_lamports == 0 {
            return 0.0;
        }
        let sol = r_sol_lamports as f64 / 1_000_000_000.0;
        let tok = r_tok_raw as f64 / 10f64.powi(tok_decimals as i32);
        sol / tok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpmm_swap_out_math() {
        let r_sol = 50_000_000_000u64; // 50 SOL
        let r_tok = 200_000_000_000_000u64; // 200,000 Tokens (6 decimals)
        let buy_sol = 100_000_000u64; // 0.10 SOL

        let tokens_out = RaydiumCpmmPoolState::calculate_swap_out(r_sol, r_tok, buy_sol, 25);
        assert!(tokens_out > 0);
        assert!(tokens_out < r_tok);

        let sol_back = RaydiumCpmmPoolState::calculate_swap_out(r_tok, r_sol, tokens_out, 25);
        // Due to 0.25% fee on each leg, roundtrip should be slightly less than 0.10 SOL
        assert!(sol_back < buy_sol);
        assert!(sol_back > (buy_sol * 99 / 100)); // Within 1% roundtrip
    }

    #[test]
    fn test_spot_price_calculation() {
        let r_sol = 50_000_000_000u64; // 50 SOL
        let r_tok = 1_000_000_000_000u64; // 1,000,000 Tokens (6 decimals)
        let price = RaydiumCpmmPoolState::calculate_price_sol(r_sol, r_tok, 6);
        assert!((price - 0.00005).abs() < 1e-8);
    }
}
