use anchor_lang::prelude::*;

    pub struct SwapResult {
        pub shares: u64,
        pub price: f64, 
    }

    pub fn calculate_amm_shares(
        amount: u64,
        yes_reserves: u64,
        no_reserves: u64,
        is_yes: bool
    ) -> Option<u64> {
        let amount_u128 = amount as u128;
        let yes_res_u128 = yes_reserves as u128;
        let no_res_u128 = no_reserves as u128;
        
        // Initial liquidity injection
        if yes_reserves == 0 && no_reserves == 0 {
            return Some(amount);
        }

        // Logic for "Bet outcome A":
        // 1. Mint `amount` of A and `amount` of B.
        // 2. Sell `amount` of B to the pool to buy A.
        //    - Pool has reserves R_A, R_B.
        //    - k = R_A * R_B.
        //    - New R_B = R_B + amount.
        //    - New R_A = k / New_R_B.
        //    - Bought A = R_A - New_R_A.
        // 3. User receives: `amount` (from step 1) + `Bought A` (from step 2).
        // 4. Pool reserves update: R_B increases by `amount`, R_A decreases by `Bought A`.
        
        // HOWEVER, the logic below implements the "Design Document" formula which is:
        // new_no = no + amount
        // new_yes = k / new_no
        // shares = yes - new_yes
        // This corresponds to Step 2 (Swapping NO for YES).
        // It returns ONLY the shares bought from the pool.
        // If we want to support "Mint + Swap", we should return `amount + shares_from_swap`.
        // BUT for now, let's stick to the Design Formula strictly as implemented below.
        
        let (pool_in, pool_out) = if is_yes {
            (no_res_u128, yes_res_u128)
        } else {
            (yes_res_u128, no_res_u128)
        };
        
        let k = pool_in.checked_mul(pool_out)?;
        let new_pool_in = pool_in.checked_add(amount_u128)?;
        // Check for div by zero? new_pool_in > 0 since amount > 0 or pool > 0.
        let new_pool_out = k.checked_div(new_pool_in)?;
        
        let shares_from_swap = pool_out.checked_sub(new_pool_out)?;
        
        Some(shares_from_swap as u64)
    }
