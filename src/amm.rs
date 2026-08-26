use crate::math::{calculate_cpmm_out, solve_optimal_arbitrage_newton};
use crate::types::{ArbitrageOpportunity, DynamicPoolState};
use chrono::Utc;

pub const BASE_GAS_FEE_SOL: f64 = 0.000005;
pub const PRIORITY_FEE_SOL: f64 = 0.00005; // 50k lamports

/// Dynamic Jito tip based on market conditions (25% of net margin, clamped [0.00005, 0.01])
fn calculate_dynamic_jito_tip(expected_profit_sol: f64) -> f64 {
    let base_tip_pct = 0.25;
    let min_tip_sol = 0.00005;
    let max_tip_sol = 0.01;

    let calculated_tip = expected_profit_sol * base_tip_pct;
    calculated_tip.clamp(min_tip_sol, max_tip_sol)
}

/// Derive CPMM reserves from USD liquidity and USD price
fn derive_pool_reserves(pool: &DynamicPoolState, sol_price_usd: f64) -> Option<(f64, f64, f64)> {
    if pool.liquidity_usd <= 0.0 || pool.price_usd <= 0.0 || sol_price_usd <= 0.0 {
        return None;
    }

    let r_sol = (pool.liquidity_usd / 2.0) / sol_price_usd;
    let r_tok = (pool.liquidity_usd / 2.0) / pool.price_usd;

    let fee_multiplier = match pool.dex.to_lowercase().as_str() {
        "pump" | "pumpswap" | "pump.fun" => 0.990,   // 1.0%
        "raydium" | "raydium_clmm" | "raydium_cpmm" => 0.9975, // 0.25%
        "meteora" | "meteora_dlmm" => 0.9980,       // 0.20%
        "orca" | "whirlpool" => 0.9970,             // 0.30%
        _ => 0.9975,
    };

    Some((r_sol, r_tok, fee_multiplier))
}

/// Evaluate optimal cross-DEX spatial arbitrage using Newton-Raphson
pub fn evaluate_optimal_arbitrage(
    mint: &str,
    symbol: &str,
    pool_a: &DynamicPoolState,
    pool_b: &DynamicPoolState,
    sol_price_usd: f64,
    min_net_profit_sol: f64,
    max_position_sol: f64,
    min_spread_pct: f64,
) -> Option<ArbitrageOpportunity> {
    if !pool_a.is_active || !pool_b.is_active {
        return None;
    }

    const MIN_LIQUIDITY_USD: f64 = 5000.0;
    if pool_a.liquidity_usd < MIN_LIQUIDITY_USD || pool_b.liquidity_usd < MIN_LIQUIDITY_USD {
        return None;
    }

    // Fix 2: Exclude internal Pump.fun bonding curves from 2-leg spatial arbitrage.
    // Spatial arbitrage requires hyper-efficient fee layers (Raydium CPMM/CLMM, Meteora DLMM, Orca Whirlpools).
    // Pump.fun is dedicated to Raydium-Native Migration Sniping.
    if pool_a.dex.to_lowercase().contains("pump") || pool_b.dex.to_lowercase().contains("pump") {
        return None;
    }

    // Determine cheaper pool (we buy from) and expensive pool (we sell to)
    let (buy_pool, sell_pool) = if pool_a.price_usd < pool_b.price_usd {
        (pool_a, pool_b)
    } else {
        (pool_b, pool_a)
    };

    let (r_sol_buy, r_tok_buy, gamma_buy) = derive_pool_reserves(buy_pool, sol_price_usd)?;
    let (r_sol_sell, r_tok_sell, gamma_sell) = derive_pool_reserves(sell_pool, sol_price_usd)?;

    // Calculate price difference and spread
    let price_diff = sell_pool.price_usd - buy_pool.price_usd;
    if price_diff <= 0.0 {
        return None;
    }
    let gross_spread_pct = (price_diff / buy_pool.price_usd) * 100.0;

    // V42: raised from 20% — the MRiY calibration showed genuinely executable 14%
    // spreads our filter would have discarded. No blanket rejection: the atomic
    // slippage thresholds + freshness-aware quote gate are the real arbiters.
    const MAX_SPREAD_PCT: f64 = 50.00;
    if gross_spread_pct < min_spread_pct || gross_spread_pct > MAX_SPREAD_PCT {
        return None;
    }

    // Solve for optimal arbitrage size in SOL via Newton-Raphson
    let optimal_input_sol = solve_optimal_arbitrage_newton(
        r_sol_buy,
        r_tok_buy,
        gamma_buy,
        r_sol_sell,
        r_tok_sell,
        gamma_sell,
        50,
        1e-8,
    )?;

    // V35: Whale Scalper Dynamic Position Sizing (5% active bin depth cap, 0.020 - 0.150 SOL sliding scale)
    let min_pool_depth_sol = r_sol_buy.min(r_sol_sell);
    let pool_safe_cap = min_pool_depth_sol * 0.05; // 5% of single-side active bin reserves
    let final_swap_sol = optimal_input_sol
        .min(pool_safe_cap)
        .min(max_position_sol)
        .clamp(0.020, max_position_sol);

    if final_swap_sol < 0.020 {
        return None;
    }

    // Exact CPMM output calculation
    // Leg 1: Buy tokens on Pool A
    let tokens_out = calculate_cpmm_out(r_sol_buy, r_tok_buy, final_swap_sol, gamma_buy);
    if tokens_out <= 0.0 {
        return None;
    }

    // Leg 2: Sell tokens on Pool B
    let sol_received = calculate_cpmm_out(r_tok_sell, r_sol_sell, tokens_out, gamma_sell);
    if sol_received <= final_swap_sol {
        return None;
    }

    let gross_profit_sol = sol_received - final_swap_sol;
    let total_gas_sol = BASE_GAS_FEE_SOL + PRIORITY_FEE_SOL;
    if gross_profit_sol <= total_gas_sol {
        return None;
    }

    let jito_tip_sol = calculate_dynamic_jito_tip(gross_profit_sol - total_gas_sol);
    let net_profit_sol = gross_profit_sol - total_gas_sol - jito_tip_sol;

    if net_profit_sol < min_net_profit_sol {
        return None;
    }

    let net_spread_pct = (net_profit_sol / final_swap_sol) * 100.0;
    let gross_profit_usd = gross_profit_sol * sol_price_usd;
    let jito_tip_usd = jito_tip_sol * sol_price_usd;
    let net_profit_usd = net_profit_sol * sol_price_usd;

    let now = Utc::now();

    Some(ArbitrageOpportunity {
        timestamp_ms: now.timestamp_millis(),
        iso_time: now.to_rfc3339(),
        mint: mint.to_string(),
        symbol: symbol.to_string(),
        name: format!("{}/SOL", symbol),
        buy_dex: buy_pool.dex.clone(),
        sell_dex: sell_pool.dex.clone(),
        target_dex: sell_pool.dex.clone(),
        pair_address: sell_pool.pool_address.clone(),
        pump_price_usd: buy_pool.price_usd,
        raydium_price_usd: sell_pool.price_usd,
        gross_spread_pct: (gross_spread_pct * 100.0).round() / 100.0,
        net_spread_pct: (net_spread_pct * 100.0).round() / 100.0,
        pool_liquidity_usd: buy_pool.liquidity_usd.min(sell_pool.liquidity_usd),
        simulated_swap_sol: (final_swap_sol * 1000.0).round() / 1000.0,
        gross_profit_usd: (gross_profit_usd * 100.0).round() / 100.0,
        jito_tip_usd: (jito_tip_usd * 100.0).round() / 100.0,
        net_profit_usd: (net_profit_usd * 100.0).round() / 100.0,
        net_profit_sol: (net_profit_sol * 10000.0).round() / 10000.0,
    })
}